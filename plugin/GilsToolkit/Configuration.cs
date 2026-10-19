using System;
using Dalamud.Configuration;

namespace GilsToolkit;

[Serializable]
public sealed class Configuration : IPluginConfiguration
{
    public int Version { get; set; } = 1;

    /// <summary>Adresse du serveur, ex. https://gils.gensi.eu</summary>
    public string ServerUrl { get; set; } = "";

    /// <summary>Jeton créé dans l'app : Paramètres → Plugin Dalamud.</summary>
    public string Token { get; set; } = "";

    public bool Enabled { get; set; } = true;

    public void Save() => Plugin.PluginInterface.SavePluginConfig(this);
}
