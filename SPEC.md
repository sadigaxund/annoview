# SPEC

Checklist for the Rust rewrite. The reference is `legacy/shotnote.py`.
An item is ticked only after it was run on this machine or covered by a passing test.
A line starting with "Differs" records a deliberate gap and its reason.

## Entry points

- [ ] `shotnote` with no arguments starts a capture, then opens the result in the editor.
- [ ] `shotnote file.png` opens that file.
- [ ] `shotnote a.png b.png` opens one window per file.
- [ ] `cmd | shotnote -` reads an image from stdin and opens it with no path.
- [ ] A stdin image that cannot be decoded prints an error to stderr and opens nothing.
- [ ] A file that cannot be opened prints an error to stderr.
- [ ] Application id is `dev.local.Shotnote`. Windows are not unique: every launch is its own process.
- [ ] `SHOTNOTE_TIMING=1` prints milliseconds from process start to the first painted frame, then exits.

## Capture

- [ ] GNOME: xdg screenshot portal, interactive, response read from the Request signal.
- [ ] A portal result outside `~/Pictures/Screenshots` is copied into it with `new_screenshot_path()` naming.
- [ ] A cancelled portal dialog opens nothing.
- [ ] Portal failure falls back to `grim -g "$(slurp)"` writing to `new_screenshot_path()`.
- [ ] `XDG_CURRENT_DESKTOP` containing `Hyprland` uses grim and slurp directly.
- [ ] A cancelled slurp opens nothing.
- [ ] Ctrl+N and File > New screenshot hide the window, wait 250 ms, capture, then show it again.
- [ ] The application stays alive during a capture even when no window is open.
- [ ] Screenshots directory is `<XDG pictures>/Screenshots`, created on demand.
- [ ] New file name is `Screenshot from YYYY-MM-DD HH-MM-SS.png`, then `... (2).png`, `(3)` when taken.

## Window

- [ ] Window content is image plus the 256-color column plus header plus toolbar, nothing else.
- [ ] Image logical size is `ceil(px / monitor scale)`.
- [ ] Window is capped at 96% of monitor width and 92% of monitor height.
- [ ] One-time correction after the first allocation when the chrome estimate was short and the window was not capped.
- [ ] Minimum width is the larger of the toolbar width and `min(header width, 520)`.
- [ ] Title is the file name, or `Untitled`, with `• ` in the header while dirty.
- [ ] Header: File menu, undo and redo buttons, zoom button on the right.
- [ ] Zoom button shows `scale * monitor scale * 100` as a rounded percent. Click sets actual size.
- [ ] Dark canvas background `(0.14, 0.14, 0.15)`.
- [ ] Opening an image never marks it dirty.

## Toolbar

- [ ] Buttons: pencil, highlighter, text, select, shapes (dropdown arrow), separator, color swatch, eyedropper.
- [ ] All buttons are 38 px squares with 20 px icons drawn by cairo.
- [ ] Active tool has the `tool-active` style. The shapes button is active for any shape.
- [ ] Tooltips match legacy wording.
- [ ] Right-click or long-press on pencil, highlighter, text and shapes selects the tool and opens its popover.
- [ ] Shapes click opens the shape list: Line (L), Rectangle (R), Ellipse / circle (E), Triangle (G).
- [ ] The shapes button icon shows the last chosen shape.
- [ ] Color swatch click opens a color dialog without alpha.
- [ ] Starting tool is always the pencil.
- [ ] Popover close returns focus to the text view when editing, else the canvas.

## Popovers

- [ ] Pencil: Size slider 1..40.
- [ ] Highlighter: Thickness 4..80 and Opacity % 10..100.
- [ ] Text: font family dropdown with search, Size spin 8..400, Bold toggle.
- [ ] Shapes: Stroke slider 1..40 and Fill shapes check.
- [ ] Every change applies live to the canvas and is saved after a 600 ms delay.

## Colors

- [ ] 256 colors: 16 grays then 16 hue families of 15 shades, identical to `build_palette`.
- [ ] Column is 28 by 18 px cells, scrolls vertically when it does not fit, no horizontal scroll.
- [ ] Selected color is outlined black then white. A color not in the palette shows no outline.
- [ ] Palette scrolls to keep the selected color visible, and centers it once after the window maps.
- [ ] Pencil, shapes and text share `pen_rgb`. Highlighter uses `hl_rgb`.
- [ ] Eyedropper tool: press and drag sample the image pixel (un-premultiplied), release returns to the previous tool.
- [ ] Key `i` and the eyedropper button start picking. Escape cancels picking.

## Tools

- [ ] Pencil: smoothed quadratic path, round caps and joins, single point draws a dot, width is the pen size.
- [ ] Highlighter: width is hl size, painted as a group with multiply blend at the stroke's own alpha, alpha stored per stroke.
- [ ] Points closer than `0.7 / (scale * monitor scale)` image px are skipped.
- [ ] Line, rectangle, ellipse, triangle: drag from corner to corner, stroke width is shape size, optional fill with the same color.
- [ ] A shape with identical start and end is discarded.
- [ ] Shift on rectangle, ellipse and triangle makes the box square. Shift on line snaps to 45 degree steps.
- [ ] Text: key `t` or the button. Click places a box. Enter commits, Shift+Enter adds a newline, Escape cancels.
- [ ] The text box grows live with the content, font family, size and bold. It has a dashed blue outline.
- [ ] Clicking elsewhere while typing commits the text. Blank text is dropped.
- [ ] Select: drag makes a marching-ants rectangle clamped to the image. A rectangle under 2 px is dropped.
- [ ] Drag inside the selection moves it and fills the old area with the ring background color.
- [ ] Ctrl+drag inside the selection copies instead.
- [ ] Delete or Backspace removes the selection and fills it with the ring background color.
- [ ] Ctrl+A selects everything and switches to the select tool.
- [ ] Escape clears the selection. Changing tool clears it unless the new tool is select.
- [ ] Cursor: crosshair, text cursor for text, move over a selection, grab while Space is held, grabbing while panning.
- [ ] Keys: `p` pen, `h` highlighter, `t` text, `s` select, `l` line, `r` rectangle, `e` or `o` ellipse, `g` triangle, `i` pick.
- [ ] Tool keys are ignored with Shift held and while typing in a text view or entry.

## History

- [ ] Ctrl+Z undo, Ctrl+Y or Ctrl+Shift+Z redo, header buttons with the same actions.
- [ ] Each edit stores only the changed pixel rectangle, before and after.
- [ ] History is limited to 1.2 GB, oldest entries dropped first, always keeping the newest.
- [ ] A new edit clears redo.
- [ ] Undo and redo clear the selection and mark the image dirty.
- [ ] Undo and redo buttons are disabled when their stack is empty.

## View

- [ ] Image is a GPU texture drawn through the snapshot. It is re-uploaded only after an edit changed pixels.
- [ ] Texture filter: nearest at effective scale 3 or more, linear from 1 to 3, trilinear below 1.
- [ ] Image origin is pixel-snapped to the monitor scale.
- [ ] In-progress stroke, shape, floating selection, text and selection outline are drawn per frame with cairo.
- [ ] The live highlighter blends with multiply over the image.
- [ ] Initial scale is `1 / monitor scale`. Fit mode follows window resizes.
- [ ] Fit scale is `min(1 / monitor scale, view w / image w, view h / image h)`.
- [ ] Scale is clamped to 0.05..64.
- [ ] Ctrl+scroll zooms around the pointer by `exp(-dy * 0.18)`.
- [ ] Ctrl+plus, Ctrl+equals, Ctrl+keypad plus zoom in by 1.25. Ctrl+minus and keypad minus zoom out.
- [ ] Pinch zooms around the gesture center.
- [ ] Zoom animates: exponential approach with rate 22 per second, step capped at 50 ms, done within 0.1 percent.
- [ ] Ctrl+0 fits. Ctrl+1 shows actual size. Both zoom around the view center.
- [ ] Scroll pans (wheel step 60 px, touchpad 1 px). Shift turns vertical scroll into horizontal.
- [ ] Middle-button drag and Space+drag pan.
- [ ] Pan is constrained: centered when smaller than the view, otherwise at most 60 px of margin past each edge.
- [ ] A text box in progress follows pan and zoom.

## Files

- [ ] Ctrl+S on an image from a file or capture writes a sibling `<name> (2).png`, or `(3)` when taken.
- [ ] A later Ctrl+S in the same window overwrites that same copy.
- [ ] An original is never overwritten.
- [ ] A non-PNG source saves as PNG with a `.png` extension.
- [ ] A stdin image saves as a new file in the screenshots directory.
- [ ] Save writes `<path>.tmp` then renames, so a failed write leaves the target intact.
- [ ] A failed save shows `Save failed: <reason>` in the toast and keeps the image dirty.
- [ ] Ctrl+Shift+S opens Save As, starting in the file's folder with its name. The result is saved as PNG, appending `.png` when missing.
- [ ] After Save As, Ctrl+S overwrites the chosen file.
- [ ] Ctrl+O opens an image in a new window, starting in the screenshots folder, filter `image/*`.
- [ ] Ctrl+C copies the selection, or the whole image. It pipes PNG to `wl-copy -t image/png`, and falls back to the GTK clipboard.
- [ ] Pending text is committed before save and copy.
- [ ] Closing a dirty image asks Save changes? with Cancel, Discard and Save. Save is the default, Cancel is the close response.
- [ ] Closing a clean image saves settings and closes at once.
- [ ] Ctrl+W and Ctrl+Q close the window.
- [ ] Toast: pill over the bottom of the image, 18 px from the bottom, no close button, 1.3 s, ignores pointer input, does not resize the image pane.
- [ ] Toast texts: `Saved`, `Copied`, `Save failed: ...`.

## Viewer (new)

- [ ] Right arrow and Page Down open the next image in the folder. Left arrow and Page Up open the previous.
- [ ] Arrow keys are ignored while a selection exists or text is being edited. Page keys always work.
- [ ] Order is case-insensitive natural order of file names, wrapping at both ends.
- [ ] Folder list holds png, jpg, jpeg, webp, gif, bmp, tif, tiff.
- [ ] Moving to another image with unsaved edits asks Save changes? first.
- [ ] `[` rotates left and `]` rotates right by 90 degrees.
- [ ] `f` flips horizontally and `v` flips vertically.
- [ ] Rotate and flip mark the image dirty and can be undone and redone.
- [ ] F11 toggles fullscreen. Escape leaves fullscreen when nothing else uses it.
- [ ] Ctrl+I and File > Details show name, size, dimensions and path.
- [ ] File > Move to Trash uses `Gio::File::trash`, then shows the next image or closes when none is left.
- [ ] Move to Trash is disabled for an image with no path.
- [ ] Formats shown: whatever GdkTexture loads (png, jpeg, webp, gif first frame, bmp, tiff).

## Settings

- [ ] File is `~/.config/shotnote/settings.json` (GLib user config dir).
- [ ] Keys and defaults: `pen_rgb` `#xxxxxx` (palette 22), `hl_rgb` (palette 67), `pen_size` 5, `hl_size` 24, `hl_alpha` 80, `shape_size` 4, `shape` `rect`, `fill` false, `font_family` `Sans`, `font_size` 28, `bold` false.
- [ ] Unknown keys are dropped. A missing or malformed file gives defaults.
- [ ] A bad color string falls back to its default. A bad shape name falls back to `rect`.
- [ ] Saved after a 600 ms pause and when a clean window closes.
- [ ] The legacy file on this machine loads unchanged.

## Menu

- [ ] File menu sections: New screenshot, Open; Save, Save As; Copy; Zoom to fit, Actual size.
- [ ] New sections: Next, Previous, Rotate left, Rotate right, Flip horizontal, Flip vertical, Fullscreen; Details, Move to Trash.
- [ ] Accelerators shown in the menu match the keys above.
- [ ] No menu label says "overwrite".

## Keys while typing

- [ ] With a text view or entry focused, only Ctrl+Z, Y, A, C, V, X pass through. Other Ctrl combos are ignored. Plain keys type.

## Build and packaging

- [ ] Cargo project at the repo root, binary `shotnote`, `Cargo.lock` committed.
- [ ] `flake.nix` with `packages.default`, `apps.default`, `devShells.default`.
- [ ] Runtime PATH has `wl-clipboard`, `grim`, `slurp`.
- [ ] Desktop file `dev.local.Shotnote.desktop`, MimeType png, jpeg, webp, gif, bmp, tiff, `Exec=shotnote %f`.
- [ ] `nix build` and `nix flake check` pass.
- [ ] `cargo test` covers naming, save-copy rule, settings, palette, history, zoom math, folder order, pixel helpers.
- [ ] Each test group was seen failing on a planted defect.
- [ ] Startup timing for the largest screenshot, five runs, recorded in the report.
- [ ] README, CHANGELOG, LICENSE, AGENTS.md.

## Known differences from legacy

- Differs: Save never overwrites an opened original. Reason: owner decision.
- Differs: Save As and Open dialogs use `gtk::FileDialog` as legacy does, so the portal decides their look.
