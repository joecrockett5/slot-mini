slot-mini for Windows
=====================

slot's cartridge shelf in front of your own RetroArch. It plays Game Boy,
Game Boy Color and Game Boy Advance games from a library folder laid out
like an Onion SD card (Roms/GB, Roms/GBC, Roms/GBA, Saves, Labels).

Start
    Unzip anywhere and run slot-mini-pc.exe. There is no installer.

    Windows may say "Windows protected your PC": the exe is not signed.
    Choose "More info", then "Run anyway".

    You need RetroArch (https://www.retroarch.com), the standalone
    version, not installed by slot-mini. It is looked for in the usual
    places; MENU -> Folders points at it if it is somewhere else. Cores
    that are missing can be fetched from MENU.

    The first time, the library is empty. Press A to pair your Miyoo and
    bring its library over by Wi-Fi (see Sync), press START (Space) to
    copy the games, saves, labels and cart shells from a Miyoo's SD card,
    or put games in the library folder's Roms/GB, Roms/GBC and Roms/GBA
    yourself. Nothing on the card is changed. MENU -> Folders shows and
    moves the library.

Sync
    slot-mini keeps this library in step with your Miyoo's over Wi-Fi:
    games, labels, cart shells, favourites and saves. The Miyoo needs a
    slot-mini with Sync in its MENU, on the same network as this PC.

    Windows Firewall asks about slot-mini the first time it runs. Choose
    Allow, with Private networks ticked. If you chose Cancel, the Miyoo
    cannot find this PC: open Windows Security, Firewall & network
    protection, Allow an app through firewall, Change settings, and tick
    slot-mini-pc under Private. (TCP and UDP port 24735.)

    Pair: MENU -> Settings -> Sync -> Pair a Miyoo, then on the Miyoo
    MENU -> Sync and pick this PC. Confirm on both only if the two codes
    match. Then choose Sync now on the Miyoo.

    From then on Sync now on either brings them in step, and the PC
    asks the Miyoo by itself when you close a game here. The icon left
    of the clock shows a sync running, a conflict or a failure.

    A game played on both since the last sync is a conflict: the later
    save is in use, the other is kept, and the cart wears a "!". Sync ->
    Conflicts shows both and can swap them.

    Delete, a right click on the selected cart, or MENU -> Remove this
    game removes a game here and on every paired Miyoo. It asks first.
    Saves are moved to Saves/slot-mini-removed, never deleted.

Controls                    Controller              Keyboard
    Browse                  D-pad or stick          Arrow keys
    Jump a letter           Up / Down (shelf)       Page Up / Page Down
    Play                    A                       Enter or Z
    Back                    B                       X
    Heart / unheart         Y                       A
    Switch system           L1 / R1                 Q / W
    Grid / shelf            R2                      2 or Tab
    Choose the core         START (Menu)            Space
    Cart studio             X                       Backspace
    Off Recents             A stick click           S
    MENU                    SELECT (Back/View)      Esc or F1
    Remove everywhere       (MENU)                  Delete
    Full screen                                     F11 or Alt+Enter

    A, B, X and Y go by an Xbox controller's labels (A at the bottom).
    MENU -> Settings -> Controller switches to the Miyoo's layout (A on
    the right). The bar above the footer lists these for whatever you
    last touched.

    The mouse does everything too: click anywhere and the bar and the
    legends turn into buttons (hold PLAY to start a game from the
    beginning). Click a cart to pick it, double-click to play, the wheel
    browses, the right button goes back.

    A game opens full screen when slot-mini is full screen, otherwise in
    a window where slot-mini's was. In a game, RetroArch's own keys
    apply; closing RetroArch comes back to the shelf.

Where things are kept
    Settings: %APPDATA%\slot-mini-pc\config.txt
    Library: %USERPROFILE%\slot-mini unless you move it. It holds everything else, in the Miyoo's layout, so
    it can be copied to and from an SD card.

Updates
    MENU -> Check for updates opens the download page when there is a
    newer version. Unzip it over the old one.

Uninstall
    Delete the folder you unzipped, and %APPDATA%\slot-mini-pc. Your
    library folder is yours and stays.

Licence
    GPL-3.0-or-later (LICENSE.txt). Built from slot by Brandon Kowalski;
    see CREDITS.txt and the licenses folder.

https://github.com/joecrockett5/slot-mini
