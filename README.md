# annoview

<img src="data/icons/io.github.sadigaxund.Annoview.svg" alt="Annoview logo" width="96">

*Take a screenshot, annotate it, keep the notes. Also an image viewer.*

Annoview opens a screenshot straight in an editor with pencil, highlighter, text, shapes and area select.
It is built with GTK4 and libadwaita, and runs on GNOME and Hyprland.

```console
$ annoview
```

## Install

Nix, with flakes:

```console
nix run github:sadigaxund/annoview -- some-image.png
```

```console
nix profile install github:sadigaxund/annoview
```

The package puts `wl-clipboard`, `grim` and `slurp` on the program's `PATH`.
It installs the desktop entry `io.github.sadigaxund.Annoview.desktop` for png, jpeg, webp, gif, bmp and tiff.

## Usage

```console
annoview                # capture, then annotate
annoview file.png       # open a file
cmd | annoview -        # read an image from stdin
```

Capture uses the xdg screenshot portal on GNOME.
On Hyprland, or when the portal fails, it runs `grim -g "$(slurp)"`.
Bind it to a key in your desktop, for example `bind = SUPER SHIFT, S, exec, annoview` in `hyprland.conf`.

Ctrl+S never overwrites the file you opened.
It writes a copy named `<name> (2).png` next to it, and later saves in the same window update that copy.
An image from stdin is saved in `~/Pictures/Screenshots`.

Right-click a tool button for its settings: size, opacity, font, fill.
Settings are kept in `~/.config/annoview/settings.json`.

## Keys

| Key | Does |
|---|---|
| `P` `H` `T` `S` | Pencil, highlighter, text, select |
| `L` `R` `E` or `O` `G` | Line, rectangle, ellipse, triangle |
| `I` | Pick a color from the image |
| `Shift` while drawing | Square, circle or 45 degree line |
| `Enter`, `Shift+Enter`, `Esc` | Place text, new line, cancel |
| `Delete` | Remove the selection |
| `Ctrl+drag` in a selection | Copy it |
| `Ctrl+A` | Select all |
| `Ctrl+Z`, `Ctrl+Y` | Undo, redo |
| `Ctrl+S`, `Ctrl+Shift+S` | Save a copy, Save As |
| `Ctrl+C` | Copy the image or the selection |
| `Ctrl+O`, `Ctrl+N` | Open, new screenshot |
| `Ctrl+W`, `Ctrl+Q` | Close |
| `Ctrl+scroll`, `Ctrl+plus`, `Ctrl+minus` | Zoom |
| `Ctrl+0`, `Ctrl+1` | Fit, actual size |
| Scroll, middle drag, `Space` drag | Pan |
| `Right` or `Page Down` | Next image in the folder |
| `Left` or `Page Up` | Previous image in the folder |
| `[` `]` | Rotate left, right |
| `F` `V` | Flip horizontally, vertically |
| `F11` | Fullscreen |
| `Ctrl+I` | File details |

Arrow keys switch images only while nothing is selected or being typed.
Move to Trash is in the File menu.

## Build from source

```console
nix develop
cargo run --release -- some-image.png
cargo test
```

`ANNOVIEW_TIMING=1` prints the milliseconds to the first painted frame and exits.

## License

MIT, see [LICENSE](./LICENSE).
