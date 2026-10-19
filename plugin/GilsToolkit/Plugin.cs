using Dalamud.Game.Command;
using Dalamud.Interface.Windowing;
using Dalamud.IoC;
using Dalamud.Plugin;
using Dalamud.Plugin.Services;

namespace GilsToolkit;

public sealed class Plugin : IDalamudPlugin
{
    [PluginService] internal static IDalamudPluginInterface PluginInterface { get; private set; } = null!;
    [PluginService] internal static IFramework Framework { get; private set; } = null!;
    [PluginService] internal static IClientState ClientState { get; private set; } = null!;
    [PluginService] internal static IPlayerState PlayerState { get; private set; } = null!;
    [PluginService] internal static IGameInventory GameInventory { get; private set; } = null!;
    [PluginService] internal static IChatGui ChatGui { get; private set; } = null!;
    [PluginService] internal static IMarketBoard MarketBoard { get; private set; } = null!;
    [PluginService] internal static IDataManager DataManager { get; private set; } = null!;
    [PluginService] internal static ICommandManager CommandManager { get; private set; } = null!;
    [PluginService] internal static IPluginLog Log { get; private set; } = null!;

    private const string Command = "/gils";

    internal static Configuration Config { get; private set; } = null!;
    internal Sender Sender { get; }
    internal Collector Collector { get; }

    private readonly WindowSystem windows = new("GilsToolkit");
    private readonly ConfigWindow configWindow;

    public Plugin()
    {
        Config = PluginInterface.GetPluginConfig() as Configuration ?? new Configuration();
        Sender = new Sender();
        Collector = new Collector(Sender);
        configWindow = new ConfigWindow(this);
        windows.AddWindow(configWindow);

        CommandManager.AddHandler(Command, new CommandInfo((_, _) => configWindow.Toggle())
        {
            HelpMessage = "Ouvre les réglages de Gils Toolkit (serveur, jeton, état de la synchronisation).",
        });
        PluginInterface.UiBuilder.Draw += windows.Draw;
        PluginInterface.UiBuilder.OpenConfigUi += configWindow.Toggle;
        PluginInterface.UiBuilder.OpenMainUi += configWindow.Toggle;
    }

    public void Dispose()
    {
        PluginInterface.UiBuilder.Draw -= windows.Draw;
        PluginInterface.UiBuilder.OpenConfigUi -= configWindow.Toggle;
        PluginInterface.UiBuilder.OpenMainUi -= configWindow.Toggle;
        CommandManager.RemoveHandler(Command);
        windows.RemoveAllWindows();
        Collector.Dispose();
        Sender.Dispose();
    }
}
