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

    The first time, the library is empty: press A to copy the games,
    saves, labels and cart shells from a Miyoo's SD card, or put games in
    the library folder's Roms/GB, Roms/GBC and Roms/GBA yourself. Nothing
    on the card is changed. MENU -> Folders shows and moves the library.

Controls                    Controller              Keyboard
    Browse                  D-pad or stick          Arrow keys
    Jump a letter           Up / Down (shelf)       Page Up / Page Down
    Play                    A                       Enter or Z
    Back                    B                       X
    Heart / unheart         Y                       A
    Switch system           L1 / R1                 Q / W
    Grid / shelf            R2                      2 or Tab
    Choose the core         START (Menu)            Space
    Cart studio             SELECT (Back/View)      Backspace
    MENU                    Guide or a stick click  Esc or F1
    Full screen                                     F11 or Alt+Enter

    A and B follow the Miyoo's layout (A on the right). MENU ->
    Controller swaps them for an Xbox layout. The mouse works too: click
    a cart to pick it, double-click to play, the wheel browses.

    In a game, RetroArch's own keys apply; closing RetroArch comes back
    to the shelf.

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
