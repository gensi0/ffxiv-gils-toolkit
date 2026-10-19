using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Dalamud.Game.Inventory;
using Dalamud.Game.Network.Structures;
using Dalamud.Game.Text;
using Dalamud.Game.Text.SeStringHandling.Payloads;
using Dalamud.Game.Chat;
using Dalamud.Plugin.Services;
using FFXIVClientStructs.FFXIV.Client.Game;
using Lumina.Excel.Sheets;

namespace GilsToolkit;

/// <summary>
/// Lit l'état du jeu (lecture seule) et prépare les envois. Tout ce qui touche
/// à la mémoire du jeu se fait sur le thread du framework (Framework.Update).
/// </summary>
public sealed unsafe class Collector : IDisposable
{
    /// <summary>Artisans (8 à 15) et récolteurs (16 à 18).</summary>
    private static readonly uint[] Jobs = [8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18];

    private static readonly GameInventoryType[] PlayerBags =
    [
        GameInventoryType.Inventory1, GameInventoryType.Inventory2,
        GameInventoryType.Inventory3, GameInventoryType.Inventory4,
        GameInventoryType.Currency, GameInventoryType.Crystals,
    ];

    private static readonly GameInventoryType[] RetainerBags =
    [
        GameInventoryType.RetainerPage1, GameInventoryType.RetainerPage2,
        GameInventoryType.RetainerPage3, GameInventoryType.RetainerPage4,
        GameInventoryType.RetainerPage5, GameInventoryType.RetainerPage6,
        GameInventoryType.RetainerPage7, GameInventoryType.RetainerCrystals,
    ];

    private readonly Sender sender;
    private DateTime nextTick = DateTime.MinValue;
    private DateTime nextSlowTick = DateTime.MinValue;
    private readonly Dictionary<ulong, string> sentMarkets = new();
    private readonly Dictionary<ulong, string> sentRetainerInventories = new();
    private string? pendingMarket;
    private string sentInventory = "";
    private string sentRetainers = "";
    private string sentLevels = "";
    private readonly Dictionary<uint, IMarketBoardPurchaseHandler> purchaseRequests = new();

    public Collector(Sender sender)
    {
        this.sender = sender;
        Plugin.Framework.Update += OnUpdate;
        Plugin.ChatGui.ChatMessage += OnChat;
        Plugin.MarketBoard.PurchaseRequested += OnPurchaseRequested;
        Plugin.MarketBoard.ItemPurchased += OnItemPurchased;
        Plugin.ClientState.Login += ResetSent;
    }

    public void Dispose()
    {
        Plugin.Framework.Update -= OnUpdate;
        Plugin.ChatGui.ChatMessage -= OnChat;
        Plugin.MarketBoard.PurchaseRequested -= OnPurchaseRequested;
        Plugin.MarketBoard.ItemPurchased -= OnItemPurchased;
        Plugin.ClientState.Login -= ResetSent;
    }

    /// <summary>Tout renvoyer au prochain passage (connexion, bouton « Envoyer »).</summary>
    public void ResetSent()
    {
        sentMarkets.Clear();
        sentRetainerInventories.Clear();
        sentInventory = sentRetainers = sentLevels = "";
        nextSlowTick = DateTime.MinValue;
    }

    private static long Now() => DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();

    private static string Key<T>(T value) => JsonSerializer.Serialize(value);

    private CharacterInfo? Character()
    {
        var ps = Plugin.PlayerState;
        if (!Plugin.ClientState.IsLoggedIn || !ps.IsLoaded) return null;
        return new CharacterInfo(ps.CharacterName, ps.HomeWorld.RowId, ps.CurrentWorld.RowId, ps.ContentId);
    }

    private void OnUpdate(IFramework framework)
    {
        var now = DateTime.UtcNow;
        if (now < nextTick || !Plugin.Config.Enabled) return;
        nextTick = now.AddSeconds(1);
        var character = Character();
        if (character == null) return;
        sender.Character = character;

        try
        {
            ReadActiveRetainer();
            if (now >= nextSlowTick)
            {
                nextSlowTick = now.AddSeconds(20);
                ReadRetainerList();
                ReadInventory();
                ReadJobLevels();
            }
        }
        catch (Exception e)
        {
            Plugin.Log.Warning(e, "Lecture de l'état du jeu impossible");
        }
    }

    private static List<ItemCount> Aggregate(IEnumerable<GameInventoryType> bags)
    {
        var counts = new Dictionary<(uint, bool), long>();
        foreach (var bag in bags)
        {
            foreach (var item in Plugin.GameInventory.GetInventoryItems(bag))
            {
                if (item.IsEmpty || item.Quantity <= 0) continue;
                var key = (item.BaseItemId, item.IsHq);
                counts[key] = counts.GetValueOrDefault(key) + item.Quantity;
            }
        }
        return counts
            .Select(kv => new ItemCount(kv.Key.Item1, kv.Key.Item2, kv.Value))
            .OrderBy(i => i.ItemId)
            .ThenBy(i => i.Hq)
            .ToList();
    }

    /// <summary>Annonces et inventaire du servant appelé (sonnette).</summary>
    private void ReadActiveRetainer()
    {
        var manager = RetainerManager.Instance();
        if (manager == null || !manager->IsReady) return;
        var retainer = manager->GetActiveRetainer();
        if (retainer == null || retainer->RetainerId == 0) return;
        var inventory = InventoryManager.Instance();
        if (inventory == null) return;
        var id = retainer->RetainerId;
        var name = retainer->NameString;

        var market = inventory->GetInventoryContainer(InventoryType.RetainerMarket);
        if (market != null && market->IsLoaded)
        {
            var listings = new List<RetainerListing>();
            var ready = true;
            foreach (var item in Plugin.GameInventory.GetInventoryItems(GameInventoryType.RetainerMarket))
            {
                if (item.IsEmpty) continue;
                var price = (long)inventory->GetRetainerMarketPrice((short)item.InventorySlot);
                if (price <= 0)
                {
                    ready = false;
                    break;
                }
                listings.Add(new RetainerListing((int)item.InventorySlot, item.BaseItemId, item.IsHq, item.Quantity, price));
            }
            // Nombre d'annonces connu du jeu : la liste doit être complète.
            if (ready && listings.Count == retainer->MarketItemCount)
            {
                var key = Key(listings);
                // Deux lectures identiques de suite : les prix sont chargés.
                if (pendingMarket == key && sentMarkets.GetValueOrDefault(id) != key)
                {
                    sentMarkets[id] = key;
                    sender.Queue(p => (p.RetainerMarkets ??= []).Add(new RetainerMarket(id, name, listings, Now())));
                    Plugin.Log.Debug($"Annonces de {name} : {listings.Count}");
                }
                pendingMarket = key;
            }
        }

        var bags = RetainerBags.Where(b =>
        {
            var c = inventory->GetInventoryContainer((InventoryType)(uint)b);
            return c != null && c->IsLoaded;
        }).ToList();
        if (bags.Count > 0)
        {
            var items = Aggregate(bags);
            var key = Key(items);
            if (sentRetainerInventories.GetValueOrDefault(id) != key)
            {
                sentRetainerInventories[id] = key;
                sender.Queue(p => (p.RetainerInventories ??= []).Add(new RetainerInventory(id, name, items, Now())));
            }
        }
    }

    private void ReadRetainerList()
    {
        var manager = RetainerManager.Instance();
        if (manager == null || !manager->IsReady) return;
        var list = new List<RetainerInfo>();
        var count = manager->GetRetainerCount();
        for (uint i = 0; i < count; i++)
        {
            var r = manager->GetRetainerBySortedIndex(i);
            if (r == null || r->RetainerId == 0) continue;
            list.Add(new RetainerInfo(r->RetainerId, r->NameString, (int)r->Town, r->MarketItemCount, r->MarketExpire));
        }
        if (list.Count == 0) return;
        var key = Key(list);
        if (key == sentRetainers) return;
        sentRetainers = key;
        sender.Queue(p => p.Retainers = list);
    }

    private void ReadInventory()
    {
        var items = Aggregate(PlayerBags);
        if (items.Count == 0) return;
        var key = Key(items);
        if (key == sentInventory) return;
        sentInventory = key;
        sender.Queue(p => p.Inventory = items);
    }

    private void ReadJobLevels()
    {
        var sheet = Plugin.DataManager.GetExcelSheet<ClassJob>();
        var levels = new Dictionary<uint, int>();
        foreach (var id in Jobs)
        {
            if (!sheet.TryGetRow(id, out var job)) continue;
            levels[id] = Plugin.PlayerState.GetClassJobLevel(job);
        }
        if (levels.Values.All(l => l <= 0)) return;
        var key = Key(levels);
        if (key == sentLevels) return;
        sentLevels = key;
        sender.Queue(p => p.JobLevels = levels);
    }

    /// <summary>« … a été vendu … » : message du canal des ventes de servants.</summary>
    private void OnChat(IHandleableChatMessage message)
    {
        if (!Plugin.Config.Enabled || message.LogKind != XivChatType.RetainerSale) return;
        var item = message.Message.Payloads.OfType<ItemPayload>().FirstOrDefault();
        var text = message.Message.TextValue;
        var at = Now();
        sender.Queue(p => (p.Events ??= []).Add(new GameEvent
        {
            Kind = "sale",
            Id = $"sale-{at}-{item?.ItemId ?? 0}",
            At = at,
            ItemId = item?.ItemId ?? 0,
            Hq = item?.IsHQ ?? false,
            Text = text,
        }));
    }

    private void OnPurchaseRequested(IMarketBoardPurchaseHandler request)
    {
        purchaseRequests[request.CatalogId] = request;
    }

    /// <summary>Achat confirmé par le serveur du jeu.</summary>
    private void OnItemPurchased(IMarketBoardPurchase purchase)
    {
        if (!Plugin.Config.Enabled) return;
        if (!purchaseRequests.Remove(purchase.CatalogId, out var request)) return;
        var at = Now();
        var world = Plugin.PlayerState.IsLoaded ? Plugin.PlayerState.CurrentWorld.RowId : 0;
        sender.Queue(p => (p.Events ??= []).Add(new GameEvent
        {
            Kind = "purchase",
            Id = $"purchase-{request.ListingId}",
            At = at,
            ItemId = purchase.CatalogId,
            Hq = request.IsHq,
            Quantity = (int)purchase.ItemQuantity,
            PricePerUnit = request.PricePerUnit,
            TotalTax = request.TotalTax,
            WorldId = world,
        }));
    }
}
