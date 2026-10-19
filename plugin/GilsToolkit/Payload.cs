using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace GilsToolkit;

// Données envoyées au serveur (POST /api/plugin/sync, JSON camelCase).
// Chaque section est facultative : seul ce qui a changé est envoyé.

public sealed class SyncPayload
{
    public CharacterInfo? Character { get; set; }
    public long SentAt { get; set; }

    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public List<ItemCount>? Inventory { get; set; }

    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public List<RetainerInfo>? Retainers { get; set; }

    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public List<RetainerMarket>? RetainerMarkets { get; set; }

    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public List<RetainerInventory>? RetainerInventories { get; set; }

    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public Dictionary<uint, int>? JobLevels { get; set; }

    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public List<GameEvent>? Events { get; set; }
}

public sealed record CharacterInfo(string Name, uint HomeWorldId, uint CurrentWorldId, ulong ContentId);

public sealed record ItemCount(uint ItemId, bool Hq, long Count);

public sealed record RetainerInfo(
    ulong RetainerId,
    string Name,
    int Town,
    int MarketItemCount,
    long MarketExpire);

public sealed record RetainerListing(int Slot, uint ItemId, bool Hq, int Quantity, long Price);

public sealed record RetainerMarket(ulong RetainerId, string Name, List<RetainerListing> Listings, long At);

public sealed record RetainerInventory(ulong RetainerId, string Name, List<ItemCount> Items, long At);

/// <summary>
/// Vente d'un servant (message de chat) ou achat à l'HV.
/// <c>Kind</c> : « sale » ou « purchase ».
/// </summary>
public sealed class GameEvent
{
    public string Kind { get; set; } = "";
    public string Id { get; set; } = "";
    public long At { get; set; }
    public uint ItemId { get; set; }
    public bool Hq { get; set; }
    public int Quantity { get; set; }
    public long PricePerUnit { get; set; }
    public long TotalTax { get; set; }
    public uint WorldId { get; set; }
    public string? Text { get; set; }
}
