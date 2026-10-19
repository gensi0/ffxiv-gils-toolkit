using System;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace GilsToolkit;

/// <summary>
/// Envoi au serveur, hors du thread du jeu : regroupe les changements, réessaie
/// en cas d'échec, et garde les événements (ventes, achats) sur disque tant
/// qu'ils ne sont pas reçus.
/// </summary>
public sealed class Sender : IDisposable
{
    public static readonly JsonSerializerOptions Json = new(JsonSerializerDefaults.Web);

    private readonly HttpClient http = new() { Timeout = TimeSpan.FromSeconds(20) };
    private readonly CancellationTokenSource stop = new();
    private readonly object gate = new();
    private readonly string spoolPath;
    private SyncPayload pending = new();
    private bool dirty;
    private int failures;

    public CharacterInfo? Character { get; set; }
    public DateTime? LastSuccess { get; private set; }
    public string? LastError { get; private set; }
    public string LastSummary { get; private set; } = "";

    public Sender()
    {
        http.DefaultRequestHeaders.UserAgent.ParseAdd("GilsToolkit-Dalamud/" + typeof(Sender).Assembly.GetName().Version);
        spoolPath = Path.Combine(Plugin.PluginInterface.GetPluginConfigDirectory(), "evenements.json");
        LoadSpool();
        _ = Task.Run(Loop);
    }

    public void Dispose()
    {
        stop.Cancel();
        SaveSpool();
        http.Dispose();
    }

    public void Queue(Action<SyncPayload> change)
    {
        lock (gate)
        {
            change(pending);
            dirty = true;
            if (pending.Events is { Count: > 0 }) SaveSpool();
        }
    }

    private void LoadSpool()
    {
        try
        {
            if (!File.Exists(spoolPath)) return;
            var events = JsonSerializer.Deserialize<GameEvent[]>(File.ReadAllText(spoolPath), Json);
            if (events is { Length: > 0 })
            {
                pending.Events = events.ToList();
                dirty = true;
            }
        }
        catch (Exception e)
        {
            Plugin.Log.Warning(e, "Événements en attente illisibles");
        }
    }

    private void SaveSpool()
    {
        try
        {
            lock (gate)
            {
                var events = pending.Events ?? [];
                File.WriteAllText(spoolPath, JsonSerializer.Serialize(events, Json));
            }
        }
        catch (Exception e)
        {
            Plugin.Log.Warning(e, "Sauvegarde des événements impossible");
        }
    }

    private async Task Loop()
    {
        while (!stop.IsCancellationRequested)
        {
            // 3 s entre deux envois, jusqu'à 5 min après des échecs répétés.
            var delay = TimeSpan.FromSeconds(Math.Min(300, 3 * Math.Pow(2, Math.Min(failures, 7))));
            try
            {
                await Task.Delay(delay, stop.Token);
                await Flush();
            }
            catch (OperationCanceledException)
            {
                return;
            }
            catch (Exception e)
            {
                Plugin.Log.Warning(e, "Envoi impossible");
            }
        }
    }

    private Uri? Endpoint(string path)
    {
        var url = Plugin.Config.ServerUrl.Trim().TrimEnd('/');
        if (url.Length == 0 || Plugin.Config.Token.Trim().Length == 0) return null;
        return Uri.TryCreate(url + path, UriKind.Absolute, out var uri) ? uri : null;
    }

    private HttpRequestMessage Request(HttpMethod method, Uri uri, string? body = null)
    {
        var req = new HttpRequestMessage(method, uri);
        req.Headers.Authorization = new AuthenticationHeaderValue("Bearer", Plugin.Config.Token.Trim());
        if (body != null) req.Content = new StringContent(body, Encoding.UTF8, "application/json");
        return req;
    }

    /// <summary>Envoie ce qui a changé depuis le dernier envoi.</summary>
    public async Task Flush()
    {
        if (!Plugin.Config.Enabled) return;
        var uri = Endpoint("/api/plugin/sync");
        if (uri == null) return;
        SyncPayload payload;
        lock (gate)
        {
            if (!dirty || Character == null) return;
            payload = pending;
            payload.Character = Character;
            payload.SentAt = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
            pending = new SyncPayload();
            dirty = false;
        }
        try
        {
            using var resp = await http.SendAsync(Request(HttpMethod.Post, uri, JsonSerializer.Serialize(payload, Json)), stop.Token);
            var text = await resp.Content.ReadAsStringAsync(stop.Token);
            if (!resp.IsSuccessStatusCode)
                throw new HttpRequestException($"{(int)resp.StatusCode} : {Short(text)}");
            failures = 0;
            LastSuccess = DateTime.Now;
            LastError = null;
            LastSummary = Short(text);
            SaveSpool();
        }
        catch (Exception e) when (e is not OperationCanceledException || !stop.IsCancellationRequested)
        {
            failures++;
            LastError = e.Message;
            // Remet les données non reçues, sans écraser des plus récentes.
            lock (gate)
            {
                pending.Inventory ??= payload.Inventory;
                pending.Retainers ??= payload.Retainers;
                pending.JobLevels ??= payload.JobLevels;
                if (payload.RetainerMarkets != null)
                {
                    pending.RetainerMarkets ??= [];
                    foreach (var m in payload.RetainerMarkets)
                        if (pending.RetainerMarkets.All(x => x.RetainerId != m.RetainerId))
                            pending.RetainerMarkets.Add(m);
                }
                if (payload.RetainerInventories != null)
                {
                    pending.RetainerInventories ??= [];
                    foreach (var m in payload.RetainerInventories)
                        if (pending.RetainerInventories.All(x => x.RetainerId != m.RetainerId))
                            pending.RetainerInventories.Add(m);
                }
                if (payload.Events != null)
                    pending.Events = payload.Events.Concat(pending.Events ?? []).ToList();
                dirty = true;
            }
            SaveSpool();
        }
    }

    /// <summary>Vérifie l'adresse et le jeton.</summary>
    public async Task<string> Test()
    {
        var uri = Endpoint("/api/plugin/ping");
        if (uri == null) return "Renseigne l'adresse du serveur et le jeton.";
        try
        {
            using var resp = await http.SendAsync(Request(HttpMethod.Get, uri));
            var text = await resp.Content.ReadAsStringAsync();
            if (resp.IsSuccessStatusCode)
            {
                using var doc = JsonDocument.Parse(text);
                return $"Connecté au compte « {doc.RootElement.GetProperty("username").GetString()} ».";
            }
            return (int)resp.StatusCode == 401 ? "Jeton refusé : crée-en un nouveau dans l'app." : $"Erreur {(int)resp.StatusCode} : {Short(text)}";
        }
        catch (Exception e)
        {
            return "Serveur injoignable : " + e.Message;
        }
    }

    private static string Short(string s) => s.Length > 200 ? s[..200] + "…" : s;
}
