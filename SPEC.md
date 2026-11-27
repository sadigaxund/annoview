# SPEC

Checklist for the Rust rewrite. The reference is `legacy/shotnote.py`.
An item is ticked only after it was run on this machine or covered by a passing test.
A line starting with "Differs" records a deliberate gap and its reason.

## Entry points

- [x] `shotnote` with no arguments starts a capture, then opens the result in the editor.
- [x] `shotnote file.png` opens that file.
- [x] `shotnote a.png b.png` opens one window per file.
- [x] `cmd | shotnote -` reads an image from stdin and opens it with no path.
- [x] A stdin image that cannot be decoded prints an error to stderr and opens nothing.
- [x] A file that cannot be opened prints an error to stderr.
- [x] Application id is `dev.local.Shotnote`. Windows are not unique: every launch is its own process.
- [x] `SHOTNOTE_TIMING=1` prints milliseconds from process start to the first painted frame, then exits.

## Capture

- [ ] GNOME: xdg screenshot portal, interactive, response read from the Request signal.
- [ ] A portal result outside `~/Pictures/Screenshots` is copied into it with `new_screenshot_path()` naming.
- [ ] A cancelled portal dialog opens nothing.
- [ ] Portal failure falls back to `grim -g "$(slurp)"` writing to `new_screenshot_path()`.
- [x] `XDG_CURRENT_DESKTOP` containing `Hyprland` uses grim and slurp directly.
- [x] A cancelled slurp opens nothing.
- [ ] Ctrl+N and File > New screenshot hide the window, wait 250 ms, capture, then show it again.
- [x] The application stays alive during a capture even when no window is open.
- [x] Screenshots directory is `<XDG pictures>/Screenshots`, created on demand.
- [x] New file name is `Screenshot from YYYY-MM-DD HH-MM-SS.png`, then `... (2).png`, `(3)` when taken.

## Window

- [x] Window content is image plus the 256-color column plus header plus toolbar, nothing else.
- [x] Image logical size is `ceil(px / monitor scale)`.
- [x] Window is capped at 96% of monitor width and 92% of monitor height.
- [x] One-time correction after the first allocation when the chrome estimate was short and the window was not capped.
- [x] Minimum width is the larger of the toolbar width and `min(header width, 520)`.
- [x] Title is the file name, or `Untitled`, with `• ` in the header while dirty.
- [x] Header: File menu, undo and redo buttons, zoom button on the right.
- [x] Zoom button shows `scale * monitor scale * 100` as a rounded percent. Click sets actual size.
- [x] Dark canvas background `(0.14, 0.14, 0.15)`.
- [x] Opening an image never marks it dirty.

## Toolbar

- [x] Buttons: pencil, highlighter, text, select, shapes (dropdown arrow), separator, color swatch, eyedropper.
- [x] All buttons are 38 px squares with 20 px icons drawn by cairo.
- [x] Active tool has the `tool-active` style. The shapes button is active for any shape.
- [x] Tooltips match legacy wording.
- [ ] Right-click or long-press on pencil, highlighter, text and shapes selects the tool and opens its popover.
- [x] Shapes click opens the shape list: Line (L), Rectangle (R), Ellipse / circle (E), Triangle (G).
- [x] The shapes button icon shows the last chosen shape.
- [ ] Color swatch click opens a color dialog without alpha.
- [x] Starting tool is always the pencil.
- [x] Popover close returns focus to the text view when editing, else the canvas.

## Popovers

- [x] Pencil: Size slider 1..40.
- [x] Highlighter: Thickness 4..80 and Opacity % 10..100.
- [x] Text: font family dropdown with search, Size spin 8..400, Bold toggle.
- [x] Shapes: Stroke slider 1..40 and Fill shapes check.
- [x] Every change applies live to the canvas and is saved after a 600 ms delay.

## Colors

- [x] 256 colors: 16 grays then 16 hue families of 15 shades, identical to `build_palette`.
- [x] Column is 28 by 18 px cells, scrolls vertically when it does not fit, no horizontal scroll.
- [x] Selected color is outlined black then white. A color not in the palette shows no outline.
- [x] Palette scrolls to keep the selected color visible, and centers it once after the window maps.
- [x] Pencil, shapes and text share `pen_rgb`. Highlighter uses `hl_rgb`.
- [x] Eyedropper tool: press and drag sample the image pixel (un-premultiplied), release returns to the previous tool.
- [x] Key `i` and the eyedropper button start picking. Escape cancels picking.

## Tools

- [x] Pencil: smoothed quadratic path, round caps and joins, single point draws a dot, width is the pen size.
- [x] Highlighter: width is hl size, painted as a group with multiply blend at the stroke's own alpha, alpha stored per stroke.
- [x] Points closer than `0.7 / (scale * monitor scale)` image px are skipped.
- [x] Line, rectangle, ellipse, triangle: drag from corner to corner, stroke width is shape size, optional fill with the same color.
- [x] A shape with identical start and end is discarded.
- [x] Shift on rectangle, ellipse and triangle makes the box square. Shift on line snaps to 45 degree steps.
- [x] Text: key `t` or the button. Click places a box. Enter commits, Shift+Enter adds a newline, Escape cancels.
- [x] The text box grows live with the content, font family, size and bold. It has a dashed blue outline.
- [x] Clicking elsewhere while typing commits the text. Blank text is dropped.
- [x] Select: drag makes a marching-ants rectangle clamped to the image. A rectangle under 2 px is dropped.
- [x] Drag inside the selection moves it and fills the old area with the ring background color.
- [x] Ctrl+drag inside the selection copies instead.
- [x] Delete or Backspace removes the selection and fills it with the ring background color.
- [x] Ctrl+A selects everything and switches to the select tool.
- [x] Escape clears the selection. Changing tool clears it unless the new tool is select.
- [ ] Cursor: crosshair, text cursor for text, move over a selection, grab while Space is held, grabbing while panning.
- [x] Keys: `p` pen, `h` highlighter, `t` text, `s` select, `l` line, `r` rectangle, `e` or `o` ellipse, `g` triangle, `i` pick.
- [x] Tool keys are ignored with Shift held and while typing in a text view or entry.

## History

- [x] Ctrl+Z undo, Ctrl+Y or Ctrl+Shift+Z redo, header buttons with the same actions.
- [x] Each edit stores only the changed pixel rectangle, before and after.
- [x] History is limited to 1.2 GB, oldest entries dropped first, always keeping the newest.
- [x] A new edit clears redo.
- [x] Undo and redo clear the selection and mark the image dirty.
- [x] Undo and redo buttons are disabled when their stack is empty.

## View

- [x] Image is a GPU texture drawn through the snapshot. It is re-uploaded only after an edit changed pixels.
- [ ] Texture filter: nearest at effective scale 3 or more, linear from 1 to 3, trilinear below 1.
- [ ] Image origin is pixel-snapped to the monitor scale.
- [x] In-progress stroke, shape, floating selection, text and selection outline are drawn per frame with cairo.
- [x] The live highlighter blends with multiply over the image.
- [x] Initial scale is `1 / monitor scale`. Fit mode follows window resizes.
- [x] Fit scale is `min(1 / monitor scale, view w / image w, view h / image h)`.
- [x] Scale is clamped to 0.05..64.
- [ ] Ctrl+scroll zooms around the pointer by `exp(-dy * 0.18)`.
- [x] Ctrl+plus, Ctrl+equals, Ctrl+keypad plus zoom in by 1.25. Ctrl+minus and keypad minus zoom out.
- [ ] Pinch zooms around the gesture center.
- [x] Zoom animates: exponential approach with rate 22 per second, step capped at 50 ms, done within 0.1 percent.
- [x] Ctrl+0 fits. Ctrl+1 shows actual size. Both zoom around the view center.
- [ ] Scroll pans (wheel step 60 px, touchpad 1 px). Shift turns vertical scroll into horizontal.
- [ ] Middle-button drag and Space+drag pan.
- [x] Pan is constrained: centered when smaller than the view, otherwise at most 60 px of margin past each edge.
- [x] A text box in progress follows pan and zoom.

## Files

- [x] Ctrl+S on an image from a file or capture writes a sibling `<name> (2).png`, or `(3)` when taken.
- [x] A later Ctrl+S in the same window overwrites that same copy.
- [x] An original is never overwritten.
- [x] A non-PNG source saves as PNG with a `.png` extension.
- [x] A stdin image saves as a new file in the screenshots directory.
- [x] Save writes `<path>.tmp` then renames, so a failed write leaves the target intact.
- [x] A failed save shows `Save failed: <reason>` in the toast and keeps the image dirty.
- [ ] Ctrl+Shift+S opens Save As, starting in the file's folder with its name. The result is saved as PNG, appending `.png` when missing.
- [ ] After Save As, Ctrl+S overwrites the chosen file.
- [ ] Ctrl+O opens an image in a new window, starting in the screenshots folder, filter `image/*`.
- [x] Ctrl+C copies the selection, or the whole image. It pipes PNG to `wl-copy -t image/png`, and falls back to the GTK clipboard.
- [x] Pending text is committed before save and copy.
- [x] Closing a dirty image asks Save changes? with Cancel, Discard and Save. Save is the default, Cancel is the close response.
- [ ] Closing a clean image saves settings and closes at once.
- [ ] Ctrl+W and Ctrl+Q close the window.
- [x] Toast: pill over the bottom of the image, 18 px from the bottom, no close button, 1.3 s, ignores pointer input, does not resize the image pane.
- [x] Toast texts: `Saved`, `Copied`, `Save failed: ...`.

## Viewer (new)

- [x] Right arrow and Page Down open the next image in the folder. Left arrow and Page Up open the previous.
- [x] Arrow keys are ignored while a selection exists or text is being edited. Page keys always work.
- [x] Order is case-insensitive natural order of file names, wrapping at both ends.
- [x] Folder list holds png, jpg, jpeg, webp, gif, bmp, tif, tiff.
- [x] Moving to another image with unsaved edits asks Save changes? first.
- [x] `[` rotates left and `]` rotates right by 90 degrees.
- [x] `f` flips horizontally and `v` flips vertically.
- [x] Rotate and flip mark the image dirty and can be undone and redone.
- [x] F11 toggles fullscreen. Escape leaves fullscreen when nothing else uses it.
- [x] Ctrl+I and File > Details show name, size, dimensions and path.
- [x] File > Move to Trash uses `Gio::File::trash`, then shows the next image or closes when none is left.
- [x] Move to Trash is disabled for an image with no path.
- [x] Formats shown: whatever GdkTexture loads (png, jpeg, webp, gif first frame, bmp, tiff).

## Settings

- [x] File is `~/.config/shotnote/settings.json` (GLib user config dir).
- [x] Keys and defaults: `pen_rgb` `#xxxxxx` (palette 22), `hl_rgb` (palette 67), `pen_size` 5, `hl_size` 24, `hl_alpha` 80, `shape_size` 4, `shape` `rect`, `fill` false, `font_family` `Sans`, `font_size` 28, `bold` false.
- [x] Unknown keys are dropped. A missing or malformed file gives defaults.
- [x] A bad color string falls back to its default. A bad shape name falls back to `rect`.
- [x] Saved after a 600 ms pause and when a clean window closes.
- [x] The legacy file on this machine loads unchanged.

## Menu

- [x] File menu sections: New screenshot, Open; Save, Save As; Copy; Zoom to fit, Actual size.
- [x] New sections: Next, Previous, Rotate left, Rotate right, Flip horizontal, Flip vertical, Fullscreen; Details, Move to Trash.
- [x] Accelerators shown in the menu match the keys above.
- [x] No menu label says "overwrite".

## Keys while typing

- [x] With a text view or entry focused, only Ctrl+Z, Y, A, C, V, X pass through. Other Ctrl combos are ignored. Plain keys type.

## Build and packaging

- [x] Cargo project at the repo root, binary `shotnote`, `Cargo.lock` committed.
- [x] `flake.nix` with `packages.default`, `apps.default`, `devShells.default`.
- [x] Runtime PATH has `wl-clipboard`, `grim`, `slurp`.
- [x] Desktop file `dev.local.Shotnote.desktop`, MimeType png, jpeg, webp, gif, bmp, tiff, `Exec=shotnote %f`.
- [x] `nix build` and `nix flake check` pass.
- [x] `cargo test` covers naming, save-copy rule, settings, palette, history, zoom math, folder order, pixel helpers.
- [x] Planted defects were caught in the naming, palette, settings and view tests. The folder test caught a real ordering bug.
- [x] Startup timing for the largest screenshot, five runs, recorded in the report.
- [x] README, CHANGELOG, LICENSE, AGENTS.md.

## Known differences from legacy

- Differs: Save never overwrites an opened original. Reason: owner decision.
- Differs: Save As and Open dialogs use `gtk::FileDialog` as legacy does, so the portal decides their look.

## Debug switches

- `SHOTNOTE_TIMING=1` prints milliseconds to the first painted frame and exits.
- `SHOTNOTE_TIMING=2` prints each startup stage to stderr and runs normally.
- `SHOTNOTE_DUMP=<file>` writes the window as a PNG after 1.5 s and exits.
- Building with `--features selftest` adds `SHOTNOTE_SCRIPT=<file>`, a scripted driver used to check tools, keys, save rules and viewer actions in a headless sway session.

## Not verified on this machine

- The portal capture path (GNOME interactive UI) was not run, so no capture was taken on the owner's desktop. The grim and slurp path was run in a headless sway session with a stand-in `slurp`.
- Pointer events (right-click, long-press, Ctrl+scroll, pinch, middle drag, Space drag) were not injected. The headless session has no pointer device. The handlers call the same view code that the keyboard zoom and the unit tests cover.
- The color dialog, Save As and Open dialogs were not opened.
- Ctrl+W and Ctrl+Q were not pressed.
- Fractional monitor scales were not tried. The monitor here reports scale 1.
- The texture filter choice was not compared visually.
