using System.Numerics;
using System.Threading.Tasks;
using Dalamud.Bindings.ImGui;
using Dalamud.Interface.Windowing;

namespace GilsToolkit;

public sealed class ConfigWindow : Window
{
    private readonly Plugin plugin;
    private string url;
    private string token;
    private string? testResult;

    public ConfigWindow(Plugin plugin) : base("Gils Toolkit###GilsToolkitConfig")
    {
        this.plugin = plugin;
        url = Plugin.Config.ServerUrl;
        token = Plugin.Config.Token;
        SizeConstraints = new WindowSizeConstraints { MinimumSize = new Vector2(420, 260) };
    }

    public override void Draw()
    {
        ImGui.TextWrapped("Envoie à ton serveur tes annonces (ouvre chaque servant à la sonnette), tes ventes, tes achats à l'HV, tes monnaies et tes niveaux. Lecture seule : rien n'est envoyé au jeu.");
        ImGui.Separator();

        ImGui.InputText("Adresse du serveur", ref url, 200);
        ImGui.InputText("Jeton", ref token, 200, ImGuiInputTextFlags.Password);
        ImGui.TextDisabled("Jeton : dans l'app, Paramètres → Plugin Dalamud → Créer un jeton.");

        var enabled = Plugin.Config.Enabled;
        if (ImGui.Checkbox("Synchronisation activée", ref enabled))
        {
            Plugin.Config.Enabled = enabled;
            Plugin.Config.Save();
        }

        if (ImGui.Button("Enregistrer"))
        {
            Plugin.Config.ServerUrl = url.Trim();
            Plugin.Config.Token = token.Trim();
            Plugin.Config.Save();
            plugin.Collector.ResetSent();
            testResult = "Enregistré.";
        }
        ImGui.SameLine();
        if (ImGui.Button("Tester la connexion"))
        {
            Plugin.Config.ServerUrl = url.Trim();
            Plugin.Config.Token = token.Trim();
            Plugin.Config.Save();
            testResult = "Test en cours…";
            _ = Task.Run(async () => testResult = await plugin.Sender.Test());
        }
        ImGui.SameLine();
        if (ImGui.Button("Tout renvoyer"))
        {
            plugin.Collector.ResetSent();
            testResult = "Envoi complet au prochain passage.";
        }
        if (testResult != null) ImGui.TextWrapped(testResult);

        ImGui.Separator();
        ImGui.TextDisabled($"Version {typeof(Plugin).Assembly.GetName().Version?.ToString(3)} · monnaies hors inventaire lues : {(Collector.LastCurrencyCount < 0 ? "pas encore" : Collector.LastCurrencyCount.ToString())}");
        var s = plugin.Sender;
        ImGui.Text(s.LastSuccess is { } ok ? $"Dernier envoi : {ok:HH:mm:ss}" : "Aucun envoi pour l'instant.");
        if (s.LastSummary.Length > 0) ImGui.TextDisabled(s.LastSummary);
        if (s.LastError != null)
        {
            ImGui.PushStyleColor(ImGuiCol.Text, new Vector4(1f, 0.45f, 0.45f, 1f));
            ImGui.TextWrapped("Erreur : " + s.LastError);
            ImGui.PopStyleColor();
        }
    }
}
