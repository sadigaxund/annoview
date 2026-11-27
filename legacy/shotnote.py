#!/usr/bin/env python3
"""Shotnote - fast screenshot annotator for GNOME (GTK4 + libadwaita).

  shotnote             trigger GNOME's screenshot UI, then annotate the result
  shotnote file.png    annotate an existing image (Ctrl+S overwrites it)
  cmd | shotnote -     annotate an image piped through stdin
"""
import collections
import colorsys
import io
import json
import math
import os
import shutil
import subprocess
import sys
import time

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
gi.require_version("Gdk", "4.0")
gi.require_version("Pango", "1.0")
gi.require_version("PangoCairo", "1.0")
from gi.repository import Adw, Gdk, Gio, GLib, Gtk, Pango, PangoCairo  # noqa: E402
import cairo  # noqa: E402

APP_ID = "dev.local.Shotnote"
CONF = os.path.join(GLib.get_user_config_dir(), "shotnote", "settings.json")
MAX_UNDO_BYTES = 1_200_000_000
MIN_SCALE, MAX_SCALE = 0.05, 64.0
SHAPES = ("line", "rect", "ellipse", "tri")
KEYMAP = {"i": "pick", "p": "pen", "h": "hl", "t": "text", "s": "select", "l": "line",
          "r": "rect", "e": "ellipse", "o": "ellipse", "g": "tri"}
TEXT_CSS = Gtk.CssProvider()
UI_CSS = Gtk.CssProvider()


# ----------------------------------------------------------------- helpers
def build_palette():
    """256 colors in one vertical run: 16 grays, then 16 hue families of 15 shades each."""
    cols = [(i / 15, i / 15, i / 15) for i in range(16)]
    for c in range(16):
        for r in range(15):
            if r < 7:
                s, v = 0.2 + 0.8 * r / 6, 1.0          # pastel -> pure
            else:
                s, v = 1.0, 1.0 - 0.8 * (r - 6) / 8    # pure -> dark
            cols.append(colorsys.hsv_to_rgb(c / 16, s, v))
    return cols


PALETTE = build_palette()


def parse_hex(h):
    return tuple(int(h[i:i + 2], 16) / 255 for i in (1, 3, 5))


def hexcol(rgb):
    return "#%02x%02x%02x" % tuple(int(round(v * 255)) for v in rgb)


def screenshots_dir():
    pics = GLib.get_user_special_dir(GLib.UserDirectory.DIRECTORY_PICTURES) \
        or os.path.expanduser("~/Pictures")
    return os.path.join(pics, "Screenshots")


def new_screenshot_path():
    d = screenshots_dir()
    os.makedirs(d, exist_ok=True)
    base = time.strftime("Screenshot from %Y-%m-%d %H-%M-%S")
    p, n = os.path.join(d, base + ".png"), 1
    while os.path.exists(p):
        n += 1
        p = os.path.join(d, f"{base} ({n}).png")
    return p


def ensure_in_screenshots(path):
    if os.path.dirname(os.path.abspath(path)) == screenshots_dir():
        return path
    dst = new_screenshot_path()
    shutil.copy(path, dst)
    return dst


def surface_from_texture(tex):
    w, h = tex.get_width(), tex.get_height()
    dl = Gdk.TextureDownloader.new(tex)
    dl.set_format(Gdk.MemoryFormat.B8G8R8A8_PREMULTIPLIED)
    data, stride = dl.download_bytes()
    return cairo.ImageSurface.create_for_data(
        bytearray(data.get_data()), cairo.FORMAT_ARGB32, w, h, stride)


def texture_from_surface(s):
    s.flush()
    return Gdk.MemoryTexture.new(
        s.get_width(), s.get_height(), Gdk.MemoryFormat.B8G8R8A8_PREMULTIPLIED,
        GLib.Bytes.new(bytes(s.get_data())), s.get_stride())


def make_layout(cr, text, family, size, bold):
    layout = PangoCairo.create_layout(cr)
    fd = Pango.FontDescription()
    fd.set_family(family)
    fd.set_weight(Pango.Weight.BOLD if bold else Pango.Weight.NORMAL)
    fd.set_absolute_size(size * Pango.SCALE)
    layout.set_font_description(fd)
    layout.set_text(text, -1)
    return layout


# ---------------------------------------------------------------- settings
class Settings:
    DEFAULTS = {"pen_rgb": hexcol(PALETTE[22]), "hl_rgb": hexcol(PALETTE[67]),
                "pen_size": 5, "hl_size": 24, "hl_alpha": 80, "shape_size": 4, "shape": "rect", "fill": False,
                "font_family": "Sans", "font_size": 28, "bold": False}

    def __init__(self):
        self.d = dict(self.DEFAULTS)
        try:
            with open(CONF) as f:
                self.d.update({k: v for k, v in json.load(f).items() if k in self.DEFAULTS})
        except (OSError, ValueError):
            pass
        for k in ("pen_rgb", "hl_rgb"):
            try:
                parse_hex(self.d[k])
            except (ValueError, TypeError, IndexError):
                self.d[k] = self.DEFAULTS[k]
        if self.d["shape"] not in SHAPES:
            self.d["shape"] = "rect"
        self._t = 0

    def save_soon(self):
        if not self._t:
            self._t = GLib.timeout_add(600, self._flush)

    def _flush(self):
        self._t = 0
        self.save()
        return False

    def save(self):
        try:
            os.makedirs(os.path.dirname(CONF), exist_ok=True)
            with open(CONF, "w") as f:
                json.dump(self.d, f, indent=1)
        except OSError:
            pass


# ------------------------------------------------------------------- icons
def draw_icon(cr, name, rgba):
    """Draw a 20x20 tool icon in the given foreground color."""
    cr.set_source_rgba(rgba.red, rgba.green, rgba.blue, rgba.alpha)
    cr.set_line_width(1.8)
    cr.set_line_cap(cairo.LINE_CAP_ROUND)
    cr.set_line_join(cairo.LINE_JOIN_ROUND)
    if name == "pen":
        cr.move_to(3, 17); cr.line_to(4.5, 12.5); cr.line_to(13.5, 3.5)
        cr.line_to(16.5, 6.5); cr.line_to(7.5, 15.5); cr.close_path(); cr.stroke()
        cr.move_to(4.5, 12.5); cr.line_to(7.5, 15.5); cr.stroke()
    elif name == "hl":
        cr.move_to(6, 11); cr.line_to(13, 4); cr.line_to(16, 7); cr.line_to(9, 14)
        cr.close_path(); cr.stroke()
        cr.set_line_width(3.2)
        cr.set_source_rgba(rgba.red, rgba.green, rgba.blue, rgba.alpha * 0.55)
        cr.move_to(3.5, 17.5); cr.line_to(12, 17.5); cr.stroke()
    elif name == "text":
        cr.set_line_width(2.2)
        cr.move_to(4, 4.5); cr.line_to(16, 4.5); cr.stroke()
        cr.move_to(10, 4.5); cr.line_to(10, 16.5); cr.stroke()
    elif name == "select":
        cr.set_dash([2.6, 2.6], 0)
        cr.rectangle(3.5, 3.5, 13, 13); cr.stroke()
    elif name == "line":
        cr.move_to(4, 16); cr.line_to(16, 4); cr.stroke()
    elif name == "rect":
        cr.rectangle(3.5, 5, 13, 10); cr.stroke()
    elif name == "ellipse":
        cr.save(); cr.translate(10, 10); cr.scale(7, 5.5); cr.arc(0, 0, 1, 0, 2 * math.pi)
        cr.restore(); cr.stroke()
    elif name == "tri":
        cr.move_to(10, 3.5); cr.line_to(16.5, 16); cr.line_to(3.5, 16); cr.close_path(); cr.stroke()
    elif name == "pick":
        cr.move_to(3.5, 16.5); cr.line_to(9, 11); cr.stroke()
        cr.set_line_width(4.5); cr.set_line_cap(cairo.LINE_CAP_BUTT)
        cr.move_to(9, 11); cr.line_to(14, 6); cr.stroke()
        cr.set_line_width(2.2); cr.set_line_cap(cairo.LINE_CAP_ROUND)
        cr.move_to(12, 3); cr.line_to(17, 8); cr.stroke()


class IconButton(Gtk.Button):
    """Square flat button with a cairo-drawn icon (or a color swatch)."""
    SIZE = 38

    def __init__(self, icon, tip):
        super().__init__()
        self.icon, self.dropdown, self.rgb = icon, False, (1, 0, 0)
        self.add_css_class("flat")
        self.set_size_request(self.SIZE, self.SIZE)
        self.set_valign(Gtk.Align.CENTER)
        self.set_halign(Gtk.Align.CENTER)
        self.set_tooltip_text(tip)
        self.da = Gtk.DrawingArea()
        self.da.set_content_width(20)
        self.da.set_content_height(20)
        self.da.set_draw_func(self.draw)
        self.set_child(self.da)

    def set_icon(self, name):
        self.icon = name
        self.da.queue_draw()

    def set_rgb(self, rgb):
        self.rgb = rgb
        self.da.queue_draw()

    def draw(self, a, cr, w, h):
        fg = a.get_color()
        cr.translate((w - 20) / 2, (h - 20) / 2)
        if self.icon == "swatch":
            cr.set_source_rgb(*self.rgb)
            cr.rectangle(2, 2, 16, 16)
            cr.fill_preserve()
            cr.set_source_rgba(fg.red, fg.green, fg.blue, 0.6)
            cr.set_line_width(1.2)
            cr.stroke()
            return
        draw_icon(cr, self.icon, fg)
        if self.dropdown:
            cr.set_source_rgba(fg.red, fg.green, fg.blue, fg.alpha)
            cr.move_to(14.5, 15.5); cr.line_to(19.5, 15.5); cr.line_to(17, 18.5)
            cr.close_path(); cr.fill()


def icon_widget(name):
    da = Gtk.DrawingArea()
    da.set_content_width(20)
    da.set_content_height(20)
    da.set_draw_func(lambda a, cr, w, h: (cr.translate((w - 20) / 2, (h - 20) / 2),
                                          draw_icon(cr, name, a.get_color())))
    return da


# ----------------------------------------------------------------- palette
class Palette(Gtk.DrawingArea):
    """One vertical column of all 256 colors (scrolled by its container)."""
    W, H = 28, 18

    def __init__(self, on_pick):
        super().__init__()
        self.set_content_width(self.W)
        self.set_content_height(len(PALETTE) * self.H)
        self.set_halign(Gtk.Align.CENTER)
        self.idx, self.on_pick = -1, on_pick
        self.set_draw_func(self.draw)
        g = Gtk.GestureClick()
        g.connect("pressed", self.pressed)
        self.add_controller(g)

    def draw(self, _a, cr, _w, _h):
        for i, (r, g, b) in enumerate(PALETTE):
            cr.set_source_rgb(r, g, b)
            cr.rectangle(0, i * self.H, self.W, self.H - 1)
            cr.fill()
        if self.idx >= 0:
            y = self.idx * self.H
            cr.set_line_width(2)
            cr.set_source_rgb(0, 0, 0)
            cr.rectangle(1, y + 1, self.W - 2, self.H - 3)
            cr.stroke()
            cr.set_line_width(1)
            cr.set_source_rgb(1, 1, 1)
            cr.rectangle(2, y + 2, self.W - 4, self.H - 5)
            cr.stroke()

    def pressed(self, _g, _n, _x, y):
        i = int(y // self.H)
        if 0 <= i < len(PALETTE):
            self.on_pick(PALETTE[i])

    def set_color(self, rgb):
        self.idx = -1
        for i, c in enumerate(PALETTE):
            if all(abs(a - b) < 0.003 for a, b in zip(c, rgb)):
                self.idx = i
                break
        self.queue_draw()


# ------------------------------------------------------------------ canvas
class Canvas(Gtk.DrawingArea):
    def __init__(self, win, surface, dev_scale, overlay):
        super().__init__()
        self.win, self.overlay, self.surface = win, overlay, surface
        self.w, self.h = surface.get_width(), surface.get_height()
        self.dev = dev_scale
        self.scale = self.target = 1.0 / dev_scale
        self.ox = self.oy = 0.0
        self.fit_mode = True
        self.undo, self.redo, self.undo_bytes = [], [], 0
        self.tool = "pen"
        self.color, self.size, self.hl_alpha = (1, 0, 0), 5, 0.8
        self.font_family, self.font_size, self.bold, self.fill = "Sans", 28, False, False
        self.item = self.sel = self.float = self.sel_a = None
        self.text_active, self.tv, self.text_pos = False, None, None
        self.space = self.panning = False
        self.mx = self.my = None
        self.tick_id, self.last_t, self.anchor = 0, 0, None
        self.dirty = False
        self.on_zoom = self.on_history = self.on_dirty = self.on_size = None

        self.set_hexpand(True)
        self.set_vexpand(True)
        self.set_focusable(True)
        self.set_draw_func(self.draw)
        self.connect("resize", self.on_resize)

        g = Gtk.GestureDrag()
        g.set_button(1)
        g.connect("drag-begin", self.drag_begin)
        g.connect("drag-update", self.drag_update)
        g.connect("drag-end", self.drag_end)
        self.add_controller(g)
        g2 = Gtk.GestureDrag()
        g2.set_button(2)
        g2.connect("drag-begin", lambda _g, x, y: self.pan_begin())
        g2.connect("drag-update", self.drag_update_pan)
        g2.connect("drag-end", lambda *_: setattr(self, "panning", False))
        self.add_controller(g2)
        sc = Gtk.EventControllerScroll.new(Gtk.EventControllerScrollFlags.BOTH_AXES)
        sc.connect("scroll", self.on_scroll)
        self.add_controller(sc)
        mc = Gtk.EventControllerMotion()
        mc.connect("motion", self.on_motion)
        mc.connect("leave", lambda *_: setattr(self, "mx", None))
        self.add_controller(mc)
        pz = Gtk.GestureZoom()
        pz.connect("begin", lambda *_: setattr(self, "_pz", self.scale))
        pz.connect("scale-changed", self.on_pinch)
        self.add_controller(pz)
        self.set_tool("pen")

    # ---- coordinates / view
    def to_img(self, wx, wy):
        return (wx - self.ox) / self.scale, (wy - self.oy) / self.scale

    def constrain(self):
        vw, vh = self.get_width(), self.get_height()
        iw, ih, m = self.w * self.scale, self.h * self.scale, 60
        self.ox = (vw - iw) / 2 if iw <= vw else min(m, max(vw - iw - m, self.ox))
        self.oy = (vh - ih) / 2 if ih <= vh else min(m, max(vh - ih - m, self.oy))

    def view_changed(self):
        self.queue_draw()
        if self.text_active:
            self.place_text()
        if self.on_zoom:
            self.on_zoom(self.scale)

    def fit_scale(self):
        vw, vh = max(1, self.get_width()), max(1, self.get_height())
        return min(1.0 / self.dev, vw / self.w, vh / self.h)

    def on_resize(self, _a, w, h):
        if w <= 0 or h <= 0:
            return
        if self.fit_mode:
            self.scale = self.target = self.fit_scale()
        self.constrain()
        self.view_changed()
        if self.on_size:
            self.on_size(w, h)

    def zoom_to(self, s, wx=None, wy=None):
        s = max(MIN_SCALE, min(MAX_SCALE, s))
        if wx is None:
            if self.mx is not None:
                wx, wy = self.mx, self.my
            else:
                wx, wy = self.get_width() / 2, self.get_height() / 2
        self.fit_mode = False
        self.target = s
        ix, iy = self.to_img(wx, wy)
        self.anchor = (wx, wy, ix, iy)
        if not self.tick_id:
            self.last_t = 0
            self.tick_id = self.add_tick_callback(self.tick)

    def zoom_by(self, f):
        self.zoom_to((self.target if self.tick_id else self.scale) * f)

    def tick(self, _w, clock):
        t = clock.get_frame_time() / 1e6
        dt = min(0.05, t - self.last_t) if self.last_t else 1 / 60
        self.last_t = t
        s = self.scale + (self.target - self.scale) * (1 - math.exp(-dt * 22))
        done = abs(s - self.target) < self.target * 1e-3
        if done:
            s = self.target
        self.scale = s
        wx, wy, ix, iy = self.anchor
        self.ox, self.oy = wx - ix * s, wy - iy * s
        self.constrain()
        self.view_changed()
        if done:
            self.tick_id = 0
            return GLib.SOURCE_REMOVE
        return GLib.SOURCE_CONTINUE

    def zoom_fit(self):
        self.zoom_to(self.fit_scale(), self.get_width() / 2, self.get_height() / 2)
        self.fit_mode = True

    def zoom_actual(self):
        self.zoom_to(1.0 / self.dev, self.get_width() / 2, self.get_height() / 2)

    def on_scroll(self, ctl, dx, dy):
        st = ctl.get_current_event_state()
        if st & Gdk.ModifierType.CONTROL_MASK:
            self.zoom_to((self.target if self.tick_id else self.scale) * math.exp(-dy * 0.18))
            return True
        try:
            k = 60 if ctl.get_unit() == Gdk.ScrollUnit.WHEEL else 1
        except AttributeError:
            k = 40
        if st & Gdk.ModifierType.SHIFT_MASK and dx == 0:
            dx, dy = dy, 0
        self.ox -= dx * k
        self.oy -= dy * k
        self.fit_mode = False
        self.constrain()
        self.view_changed()
        return True

    def on_pinch(self, g, s):
        _ok, x, y = g.get_bounding_box_center()
        self.zoom_to(self._pz * s, x, y)

    def pan_begin(self):
        self.panning, self.pan0 = True, (self.ox, self.oy)
        self.set_cursor_from_name("grabbing")

    def drag_update_pan(self, _g, dx, dy):
        self.ox, self.oy = self.pan0[0] + dx, self.pan0[1] + dy
        self.fit_mode = False
        self.constrain()
        self.view_changed()

    def set_space(self, on):
        self.space = on
        self.set_cursor_from_name("grab" if on else self.cursor_name())

    # ---- tools / state
    def cursor_name(self):
        return "text" if self.tool == "text" else "crosshair"

    def pick_at(self, p):
        x, y = int(max(0, min(self.w - 1, p[0]))), int(max(0, min(self.h - 1, p[1])))
        s = self.surface
        s.flush()
        d = s.get_data()
        o = y * s.get_stride() + x * 4
        b, g, r, a = d[o], d[o + 1], d[o + 2], d[o + 3]
        if 0 < a < 255:
            r, g, b = (min(255, v * 255 // a) for v in (r, g, b))
        self.win.set_color((r / 255, g / 255, b / 255))

    def set_tool(self, t):
        self.commit_text()
        if t != "select":
            self.sel = self.float = None
        self.tool = t
        self.set_cursor_from_name(self.cursor_name())
        self.queue_draw()

    def escape(self):
        if self.tool == "pick":
            self.win.finish_pick()
        elif self.text_active:
            self.close_text()
        else:
            self.sel = self.float = None
            self.queue_draw()

    def on_motion(self, _c, x, y):
        self.mx, self.my = x, y
        if self.tool == "select" and not self.space and not self.panning:
            inside = self.sel and self.in_sel(self.to_img(x, y))
            self.set_cursor_from_name("move" if inside else "crosshair")

    # ---- history
    def copy_region(self, x, y, w, h):
        s = cairo.ImageSurface(cairo.FORMAT_ARGB32, w, h)
        c = cairo.Context(s)
        c.set_operator(cairo.OPERATOR_SOURCE)
        c.set_source_surface(self.surface, -x, -y)
        c.paint()
        return s

    def commit(self, bbox, painter):
        x0, y0 = max(0, int(math.floor(bbox[0]))), max(0, int(math.floor(bbox[1])))
        x1, y1 = min(self.w, int(math.ceil(bbox[2]))), min(self.h, int(math.ceil(bbox[3])))
        if x1 <= x0 or y1 <= y0:
            return
        w, h = x1 - x0, y1 - y0
        before = self.copy_region(x0, y0, w, h)
        cr = cairo.Context(self.surface)
        cr.rectangle(x0, y0, w, h)
        cr.clip()
        painter(cr)
        self.surface.flush()
        after = self.copy_region(x0, y0, w, h)
        self.undo.append((x0, y0, w, h, before, after))
        self.undo_bytes += w * h * 8
        while self.undo_bytes > MAX_UNDO_BYTES and len(self.undo) > 1:
            o = self.undo.pop(0)
            self.undo_bytes -= o[2] * o[3] * 8
        self.redo.clear()
        self.set_dirty(True)
        self.history_changed()
        self.queue_draw()

    def apply_patch(self, op, after):
        x, y, w, h, b, a = op
        c = cairo.Context(self.surface)
        c.rectangle(x, y, w, h)
        c.clip()
        c.set_operator(cairo.OPERATOR_SOURCE)
        c.set_source_surface(a if after else b, x, y)
        c.paint()
        self.surface.flush()

    def do_undo(self):
        self.sel = self.float = None
        if self.undo:
            op = self.undo.pop()
            self.apply_patch(op, False)
            self.redo.append(op)
            self.set_dirty(True)
            self.history_changed()
            self.queue_draw()

    def do_redo(self):
        self.sel = self.float = None
        if self.redo:
            op = self.redo.pop()
            self.apply_patch(op, True)
            self.undo.append(op)
            self.set_dirty(True)
            self.history_changed()
            self.queue_draw()

    def history_changed(self):
        if self.on_history:
            self.on_history(bool(self.undo), bool(self.redo))

    def set_dirty(self, d):
        self.dirty = d
        if self.on_dirty:
            self.on_dirty()

    # ---- painting of items (used live and for commit)
    def make_item(self, kind):
        return {"k": kind, "color": self.color, "fill": self.fill,
                "width": self.size, "alpha": self.hl_alpha}

    @staticmethod
    def paint_item(cr, it):
        k, (r, g, b), wd = it["k"], it["color"], it["width"]
        cr.set_line_cap(cairo.LINE_CAP_ROUND)
        cr.set_line_join(cairo.LINE_JOIN_ROUND)
        cr.set_line_width(wd)
        if k in ("pen", "hl"):
            pts = it["pts"]
            if k == "hl":
                cr.push_group()
            cr.set_source_rgb(r, g, b)
            if len(pts) == 1:
                cr.arc(pts[0][0], pts[0][1], wd / 2, 0, 2 * math.pi)
                cr.fill()
            else:
                cr.move_to(*pts[0])
                for i in range(1, len(pts) - 1):
                    q, nx = pts[i], pts[i + 1]
                    m = ((q[0] + nx[0]) / 2, (q[1] + nx[1]) / 2)
                    p0 = cr.get_current_point()
                    cr.curve_to(p0[0] + 2 / 3 * (q[0] - p0[0]), p0[1] + 2 / 3 * (q[1] - p0[1]),
                                m[0] + 2 / 3 * (q[0] - m[0]), m[1] + 2 / 3 * (q[1] - m[1]),
                                m[0], m[1])
                cr.line_to(*pts[-1])
                cr.stroke()
            if k == "hl":
                cr.pop_group_to_source()
                cr.set_operator(cairo.OPERATOR_MULTIPLY)
                cr.paint_with_alpha(it["alpha"])
                cr.set_operator(cairo.OPERATOR_OVER)
            return
        (x0, y0), (x1, y1) = it["p0"], it["p1"]
        cr.set_source_rgb(r, g, b)
        if k == "line":
            cr.move_to(x0, y0)
            cr.line_to(x1, y1)
            cr.stroke()
            return
        if k == "rect":
            cr.rectangle(min(x0, x1), min(y0, y1), abs(x1 - x0), abs(y1 - y0))
        elif k == "ellipse":
            cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
            cr.save()
            cr.translate(cx, cy)
            cr.scale(max(abs(x1 - x0) / 2, 0.01), max(abs(y1 - y0) / 2, 0.01))
            cr.arc(0, 0, 1, 0, 2 * math.pi)
            cr.restore()
        elif k == "tri":
            cr.move_to((x0 + x1) / 2, y0)
            cr.line_to(x1, y1)
            cr.line_to(x0, y1)
            cr.close_path()
        if it["fill"]:
            cr.fill_preserve()
        cr.stroke()

    @staticmethod
    def item_bbox(it):
        pad = it["width"] * 1.5 + 2
        pts = it["pts"] if "pts" in it else [it["p0"], it["p1"]]
        xs, ys = [p[0] for p in pts], [p[1] for p in pts]
        return min(xs) - pad, min(ys) - pad, max(xs) + pad, max(ys) + pad

    def commit_item(self, it):
        if "p0" in it and it["p0"] == it["p1"]:
            return
        self.commit(self.item_bbox(it), lambda cr: self.paint_item(cr, it))

    # ---- drawing
    def draw(self, _a, cr, _vw, _vh):
        cr.set_source_rgb(0.14, 0.14, 0.15)
        cr.paint()
        d = self.get_scale_factor() or 1
        cr.save()
        cr.translate(round(self.ox * d) / d, round(self.oy * d) / d)
        cr.scale(self.scale, self.scale)
        cr.rectangle(0, 0, self.w, self.h)
        cr.clip()
        cr.set_source_surface(self.surface, 0, 0)
        eff = self.scale * d
        cr.get_source().set_filter(cairo.FILTER_NEAREST if eff >= 3
                                   else cairo.FILTER_GOOD if eff < 1 else cairo.FILTER_BILINEAR)
        cr.paint()
        if self.float:
            f, (x, y, w, h) = self.float, self.sel
            if f["bg"] is not None:
                cr.set_source_rgb(*f["bg"])
                cr.rectangle(x, y, w, h)
                cr.fill()
            cr.set_source_surface(f["surf"], x + f["dx"], y + f["dy"])
            cr.paint()
        if self.item:
            self.paint_item(cr, self.item)
        if self.text_active and self.tv:
            cr.set_source_rgb(*self.color)
            cr.move_to(*self.text_pos)
            PangoCairo.show_layout(cr, make_layout(cr, self.tv_text(), self.font_family,
                                                   self.font_size, self.bold))
        cr.restore()
        if self.text_active and self.tv:
            tw, th = self.text_extent()
            cr.set_line_width(1)
            cr.set_source_rgb(0.21, 0.52, 0.89)
            cr.set_dash([5, 4], 0)
            cr.rectangle(self.ox + self.text_pos[0] * self.scale - 2.5,
                         self.oy + self.text_pos[1] * self.scale - 2.5,
                         tw * self.scale + 5, th * self.scale + 5)
            cr.stroke()
            cr.set_dash([], 0)
        if self.sel:
            x, y, w, h = self.sel
            if self.float:
                x, y = x + self.float["dx"], y + self.float["dy"]
            sx, sy = self.ox + x * self.scale, self.oy + y * self.scale
            sw, sh = w * self.scale, h * self.scale
            cr.set_line_width(1)
            for col, off in (((0, 0, 0), 0), ((1, 1, 1), 4)):
                cr.set_source_rgb(*col)
                cr.set_dash([4, 4], off)
                cr.rectangle(sx + .5, sy + .5, sw, sh)
                cr.stroke()

    # ---- pointer input
    def drag_begin(self, g, x, y):
        self.grab_focus()
        st = g.get_current_event_state()
        if self.text_active:
            self.commit_text()
        if self.space:
            self.pan_begin()
            return
        self.w0 = (x, y)
        p = self.to_img(x, y)
        self.p_start = p
        t = self.tool
        if t in ("pen", "hl"):
            self.item = self.make_item(t)
            self.item["pts"] = [p]
        elif t == "text":
            self.start_text(p)
        elif t in SHAPES:
            self.item = self.make_item(t)
            self.item["p0"] = self.item["p1"] = p
        elif t == "pick":
            self.pick_at(p)
        elif t == "select":
            if self.sel and self.in_sel(p):
                sx, sy, sw, sh = self.sel
                copy = bool(st & Gdk.ModifierType.CONTROL_MASK)
                self.float = {"surf": self.copy_region(sx, sy, sw, sh), "dx": 0, "dy": 0,
                              "bg": None if copy else self.ring_color(sx, sy, sw, sh)}
            else:
                self.sel, self.sel_a = None, p
        self.queue_draw()

    def drag_update(self, g, dx, dy):
        if self.panning:
            self.drag_update_pan(g, dx, dy)
            return
        px, py = self.to_img(self.w0[0] + dx, self.w0[1] + dy)
        shift = bool(g.get_current_event_state() & Gdk.ModifierType.SHIFT_MASK)
        it = self.item
        if it and "pts" in it:
            lx, ly = it["pts"][-1]
            if (px - lx) ** 2 + (py - ly) ** 2 >= (0.7 / (self.scale * self.dev)) ** 2:
                it["pts"].append((px, py))
        elif it:
            x0, y0 = it["p0"]
            if shift:
                if it["k"] == "line":
                    ang = round(math.atan2(py - y0, px - x0) / (math.pi / 4)) * math.pi / 4
                    ln = math.hypot(px - x0, py - y0)
                    px, py = x0 + ln * math.cos(ang), y0 + ln * math.sin(ang)
                else:
                    m = max(abs(px - x0), abs(py - y0))
                    px, py = x0 + math.copysign(m, px - x0), y0 + math.copysign(m, py - y0)
            it["p1"] = (px, py)
        elif self.tool == "pick":
            self.pick_at((px, py))
        elif self.tool == "select":
            if self.float:
                self.float["dx"], self.float["dy"] = round(dx / self.scale), round(dy / self.scale)
            elif self.sel_a:
                ax, ay = self.sel_a
                x0, x1 = sorted((max(0, min(self.w, ax)), max(0, min(self.w, px))))
                y0, y1 = sorted((max(0, min(self.h, ay)), max(0, min(self.h, py))))
                x0, y0, x1, y1 = int(x0), int(y0), math.ceil(x1), math.ceil(y1)
                self.sel = (x0, y0, x1 - x0, y1 - y0)
        self.queue_draw()

    def drag_end(self, _g, _dx, _dy):
        if self.panning:
            self.panning = False
            self.set_cursor_from_name(self.cursor_name())
            return
        it, self.item = self.item, None
        if it:
            self.commit_item(it)
        elif self.tool == "pick":
            self.win.finish_pick()
        elif self.tool == "select":
            if self.float:
                self.finish_move()
            elif self.sel and (self.sel[2] < 2 or self.sel[3] < 2):
                self.sel = None
        self.queue_draw()

    # ---- selection
    def in_sel(self, p):
        x, y, w, h = self.sel
        return x <= p[0] < x + w and y <= p[1] < y + h

    def ring_color(self, x, y, w, h):
        """Most common color in the 1px ring just outside the rect (background guess)."""
        s = self.surface
        s.flush()
        d, st = s.get_data(), s.get_stride()
        cnt = collections.Counter()

        def px(ix, iy):
            if 0 <= ix < self.w and 0 <= iy < self.h:
                o = iy * st + ix * 4
                cnt[(d[o], d[o + 1], d[o + 2])] += 1
        step = max(1, (w + h) // 200)
        for ix in range(x, x + w, step):
            px(ix, y - 1)
            px(ix, y + h)
        for iy in range(y, y + h, step):
            px(x - 1, iy)
            px(x + w, iy)
        if not cnt:
            return (1.0, 1.0, 1.0)
        (b, g, r), _ = cnt.most_common(1)[0]
        return (r / 255, g / 255, b / 255)

    def finish_move(self):
        x, y, w, h = self.sel
        f = self.float
        nx, ny = x + f["dx"], y + f["dy"]
        self.float = None
        if (f["dx"], f["dy"]) == (0, 0):
            return

        def painter(cr):
            if f["bg"] is not None:
                cr.set_source_rgb(*f["bg"])
                cr.rectangle(x, y, w, h)
                cr.fill()
            cr.set_source_surface(f["surf"], nx, ny)
            cr.paint()
        self.commit((min(x, nx), min(y, ny), max(x, nx) + w, max(y, ny) + h), painter)
        self.sel = (nx, ny, w, h)

    def delete_selection(self):
        if not self.sel:
            return
        x, y, w, h = self.sel
        bg = self.ring_color(x, y, w, h)

        def painter(cr):
            cr.set_source_rgb(*bg)
            cr.rectangle(x, y, w, h)
            cr.fill()
        self.commit((x, y, x + w, y + h), painter)
        self.sel = None

    def select_all(self):
        self.sel = (0, 0, self.w, self.h)
        self.queue_draw()

    def copy_surface(self):
        if self.sel:
            x, y, w, h = self.sel
            x0, y0, x1, y1 = max(0, x), max(0, y), min(self.w, x + w), min(self.h, y + h)
            if x1 > x0 and y1 > y0:
                return self.copy_region(x0, y0, x1 - x0, y1 - y0)
        self.surface.flush()
        return self.surface

    # ---- text (the TextView only captures input; the canvas draws the real text + box)
    def tv_text(self):
        buf = self.tv.get_buffer()
        return buf.get_text(buf.get_start_iter(), buf.get_end_iter(), False)

    def text_extent(self, text=None):
        """(w, h) in image px of the text as it will be committed (grows with content)."""
        if text is None:
            text = self.tv_text()
        w, h = make_layout(cairo.Context(self.surface), text, self.font_family,
                           self.font_size, self.bold).get_pixel_size()
        return max(w, int(self.font_size * 0.6)), max(h, int(self.font_size * 1.2))

    def start_text(self, p):
        self.text_pos = p
        tv = Gtk.TextView()
        tv.add_css_class("shotnote-text")
        tv.set_wrap_mode(Gtk.WrapMode.NONE)
        tv.set_halign(Gtk.Align.START)
        tv.set_valign(Gtk.Align.START)
        kc = Gtk.EventControllerKey()
        kc.set_propagation_phase(Gtk.PropagationPhase.CAPTURE)
        kc.connect("key-pressed", self.on_text_key)
        tv.add_controller(kc)
        tv.get_buffer().connect("changed", lambda *_: (self.place_text(), self.queue_draw()))
        self.tv, self.text_active = tv, True
        self.overlay.add_overlay(tv)
        self.place_text()
        GLib.idle_add(lambda: (tv.grab_focus(), False)[1])

    def place_text(self):
        if not self.tv:
            return
        tw, th = self.text_extent()
        self.tv.set_margin_start(int(self.ox + self.text_pos[0] * self.scale))
        self.tv.set_margin_top(int(self.oy + self.text_pos[1] * self.scale))
        self.tv.set_size_request(int(math.ceil(tw * self.scale)) + 4, int(math.ceil(th * self.scale)))
        c = hexcol(self.color)
        fam = self.font_family.replace('"', "")
        TEXT_CSS.load_from_string(
            f'.shotnote-text, .shotnote-text text {{ background: transparent; color: transparent;'
            f' caret-color: {c}; font-family: "{fam}"; font-size: {self.font_size * self.scale:.2f}px;'
            f' font-weight: {700 if self.bold else 400}; padding: 0; margin: 0; border: none;'
            f' min-height: 0; }} .shotnote-text text selection {{ background: alpha(#3584e4, 0.35); }}')

    def on_text_key(self, _kc, keyval, _code, state):
        if keyval in (Gdk.KEY_Return, Gdk.KEY_KP_Enter) and not state & Gdk.ModifierType.SHIFT_MASK:
            self.commit_text()
            return True
        if keyval == Gdk.KEY_Escape:
            self.close_text()
            return True
        return False

    def close_text(self):
        tv, self.tv, self.text_active = self.tv, None, False
        if tv:
            GLib.idle_add(lambda: (self.overlay.remove_overlay(tv), False)[1])
        self.queue_draw()
        self.grab_focus()

    def commit_text(self):
        if not self.text_active:
            return
        text = self.tv_text()
        p, color = self.text_pos, self.color
        fam, size, bold = self.font_family, self.font_size, self.bold
        self.close_text()
        if not text.strip():
            return
        lw, lh = make_layout(cairo.Context(self.surface), text, fam, size, bold).get_pixel_size()

        def painter(cr):
            cr.set_source_rgb(*color)
            cr.move_to(*p)
            PangoCairo.show_layout(cr, make_layout(cr, text, fam, size, bold))
        self.commit((p[0] - 3, p[1] - 3, p[0] + lw + 6, p[1] + lh + 6), painter)


# ------------------------------------------------------------------ window
class Window(Adw.ApplicationWindow):
    def __init__(self, app, surface, path):
        super().__init__(application=app)
        self.app, self.cfg, self.path = app, app.settings, path
        self._corrected = self._centered = False
        self.prev_tool, self.popovers, self.tool_buttons = "pen", {}, {}

        mon = Gdk.Display.get_default().get_monitors().get_item(0)
        if mon:
            dev = mon.get_scale() if hasattr(mon, "get_scale") else mon.get_scale_factor()
            geo = mon.get_geometry()
            mw, mh = geo.width, geo.height
        else:
            dev, mw, mh = 1.0, 1920, 1080
        self.dev = dev

        # ---- header: file menu, undo/redo, title, zoom
        self.header = Adw.HeaderBar()
        self.wtitle = Adw.WindowTitle()
        self.header.set_title_widget(self.wtitle)
        self.header.pack_start(Gtk.MenuButton(label="File", menu_model=self.build_menu()))
        self.undo_btn = Gtk.Button.new_from_icon_name("edit-undo-symbolic")
        self.redo_btn = Gtk.Button.new_from_icon_name("edit-redo-symbolic")
        self.undo_btn.set_tooltip_text("Undo (Ctrl+Z)")
        self.redo_btn.set_tooltip_text("Redo (Ctrl+Y)")
        self.undo_btn.connect("clicked", lambda *_: self.canvas.do_undo())
        self.redo_btn.connect("clicked", lambda *_: self.canvas.do_redo())
        self.header.pack_start(self.undo_btn)
        self.header.pack_start(self.redo_btn)
        self.zoom_btn = Gtk.Button(label="100%")
        self.zoom_btn.add_css_class("flat")
        self.zoom_btn.set_tooltip_text("Click: actual size  |  Ctrl+0: fit")
        self.zoom_btn.connect("clicked", lambda *_: self.canvas.zoom_actual())
        self.header.pack_end(self.zoom_btn)

        # ---- canvas
        overlay = Gtk.Overlay()
        self.canvas = Canvas(self, surface, dev, overlay)
        overlay.set_child(self.canvas)
        overlay.set_hexpand(True)
        self.toast_label = Gtk.Label()
        self.toast_label.add_css_class("shotnote-toast")
        self.toast_label.set_halign(Gtk.Align.CENTER)
        self.toast_label.set_valign(Gtk.Align.END)
        self.toast_label.set_margin_bottom(18)
        self.toast_label.set_can_target(False)
        self.toast_label.set_visible(False)
        overlay.add_overlay(self.toast_label)
        self._toast_id = 0

        # ---- left: vertical 256-color strip (scrolls when it does not fit)
        self.palette = Palette(self.set_color)
        self.pal_sw = Gtk.ScrolledWindow()
        self.pal_sw.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        self.pal_sw.set_size_request(Palette.W + 10, -1)
        self.pal_sw.set_vexpand(True)
        self.pal_sw.set_child(self.palette)

        # ---- top: tool bar (all buttons the same square size)
        self.toolbar = self.build_toolbar()

        body = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        body.append(self.pal_sw)
        body.append(Gtk.Separator(orientation=Gtk.Orientation.VERTICAL))
        body.append(overlay)
        tv = Adw.ToolbarView()
        tv.add_top_bar(self.header)
        tv.add_top_bar(self.toolbar)
        tv.set_content(body)
        self.set_content(tv)

        self.canvas.on_zoom = self.zoom_changed
        self.canvas.on_history = self.history_changed
        self.canvas.on_dirty = self.update_title
        self.canvas.on_size = self.on_canvas_size
        self.history_changed(False, False)
        self.update_title()
        self.zoom_changed(self.canvas.scale)
        self.choose_tool("pen")          # always start on the pencil

        kc = Gtk.EventControllerKey()
        kc.set_propagation_phase(Gtk.PropagationPhase.CAPTURE)
        kc.connect("key-pressed", self.on_key)
        kc.connect("key-released", self.on_key_release)
        self.add_controller(kc)
        self.connect("close-request", self.on_close)
        self.connect("map", self.on_map)

        # ---- window size = image + palette column + header + toolbar (nothing else)
        ilw, ilh = math.ceil(self.canvas.w / dev), math.ceil(self.canvas.h / dev)
        V, Hz = Gtk.Orientation.VERTICAL, Gtk.Orientation.HORIZONTAL
        chrome_h = self.header.measure(V, -1)[1] + self.toolbar.measure(V, -1)[1]
        min_w = max(self.toolbar.measure(Hz, -1)[1], min(self.header.measure(Hz, -1)[1], 520))
        W = max(Palette.W + 10 + 1 + ilw, min_w)
        H = chrome_h + ilh
        self._capped = W > int(mw * 0.96) or H > int(mh * 0.92)
        self._size = (min(W, int(mw * 0.96)), min(H, int(mh * 0.92)))
        self.set_default_size(*self._size)

    # ---- construction helpers
    def build_menu(self):
        def mi(label, action, accel):
            it = Gio.MenuItem.new(label, action)
            it.set_attribute_value("accel", GLib.Variant("s", accel))
            return it
        m = Gio.Menu()
        m.append_item(mi("New screenshot", "win.new", "<Primary>n"))
        m.append_item(mi("Open…", "win.open", "<Primary>o"))
        s1 = Gio.Menu()
        s1.append_item(mi("Save (overwrite)", "win.save", "<Primary>s"))
        s1.append_item(mi("Save As…", "win.saveas", "<Primary><Shift>s"))
        s2 = Gio.Menu()
        s2.append_item(mi("Copy image / selection", "win.copy", "<Primary>c"))
        s3 = Gio.Menu()
        s3.append_item(mi("Zoom to fit", "win.fit", "<Primary>0"))
        s3.append_item(mi("Actual size", "win.actual", "<Primary>1"))
        for sec in (s1, s2, s3):
            m.append_section(None, sec)
        for name, fn in (("new", lambda: self.app.capture(self)),
                         ("open", self.open_dialog), ("save", self.save),
                         ("saveas", self.save_as), ("copy", self.copy_image),
                         ("fit", lambda: self.canvas.zoom_fit()),
                         ("actual", lambda: self.canvas.zoom_actual())):
            a = Gio.SimpleAction.new(name, None)
            a.connect("activate", lambda *_, f=fn: f())
            self.add_action(a)
        return m

    def build_toolbar(self):
        bar = Gtk.Box(spacing=4)
        bar.add_css_class("toolbar")
        specs = [("pen", "pen", "Pencil (P)  -  right-click: size"),
                 ("hl", "hl", "Highlighter (H)  -  right-click: thickness / opacity"),
                 ("text", "text", "Text (T)  -  right-click: font"),
                 ("select", "select", "Select area (S)  -  drag to move, Ctrl+drag to copy, Del to remove"),
                 ("shape", self.cfg.d["shape"], "Shapes  -  click for list, right-click: stroke / fill")]
        for key, icon, tip in specs:
            b = IconButton(icon, tip)
            self.tool_buttons[key] = b
            if key == "shape":
                b.dropdown = True
                b.connect("clicked", lambda *_: self.popover_for("shape_menu").popup())
            else:
                b.connect("clicked", lambda *_, k=key: self.choose_tool(k))
            if key in ("pen", "hl", "text", "shape"):
                rc = Gtk.GestureClick()
                rc.set_button(3)
                rc.connect("pressed", lambda g, n, x, y, k=key: (
                    g.set_state(Gtk.EventSequenceState.CLAIMED), self.show_options(k)))
                b.add_controller(rc)
                lp = Gtk.GestureLongPress()
                lp.connect("pressed", lambda g, x, y, k=key: self.show_options(k))
                b.add_controller(lp)
            bar.append(b)

        sep = Gtk.Separator(orientation=Gtk.Orientation.VERTICAL)
        sep.set_margin_top(6)
        sep.set_margin_bottom(6)
        bar.append(sep)

        self.color_btn = IconButton("swatch", "Current color  -  click for a custom color")
        self.color_btn.connect("clicked", lambda *_: self.custom_color())
        bar.append(self.color_btn)
        pk = IconButton("pick", "Pick color from the image (I)")
        pk.connect("clicked", lambda *_: self.choose_tool("pick"))
        self.tool_buttons["pick"] = pk
        bar.append(pk)
        return bar

    def slider_row(self, label, key, lo, hi):
        row = Gtk.Box(spacing=10)
        lbl = Gtk.Label(label=label, xalign=0)
        lbl.set_size_request(70, -1)
        row.append(lbl)
        sc = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, lo, hi, 1)
        sc.set_size_request(190, -1)
        sc.set_draw_value(True)
        sc.set_value_pos(Gtk.PositionType.RIGHT)
        sc.set_value(self.cfg.d[key])

        def changed(s):
            self.cfg.d[key] = int(s.get_value())
            self.cfg.save_soon()
            self.apply_params()
        sc.connect("value-changed", changed)
        row.append(sc)
        return row

    def font_rows(self):
        names = sorted({f.get_name() for f in PangoCairo.FontMap.get_default().list_families()},
                       key=str.casefold)
        dd = Gtk.DropDown(model=Gtk.StringList.new(names))
        dd.set_enable_search(True)
        dd.set_expression(Gtk.PropertyExpression.new(Gtk.StringObject, None, "string"))
        dd.set_size_request(250, -1)
        fac = Gtk.SignalListItemFactory()
        fac.connect("setup", lambda _f, li: li.set_child(
            Gtk.Label(xalign=0, ellipsize=Pango.EllipsizeMode.END, max_width_chars=26)))
        fac.connect("bind", lambda _f, li: li.get_child().set_text(li.get_item().get_string()))
        dd.set_factory(fac)
        cur = self.cfg.d["font_family"]
        if cur in names:
            dd.set_selected(names.index(cur))

        def fam(d, _p):
            it = d.get_selected_item()
            if it:
                self.cfg.d["font_family"] = it.get_string()
                self.cfg.save_soon()
                self.apply_params()
        dd.connect("notify::selected", fam)

        row = Gtk.Box(spacing=8)
        row.append(Gtk.Label(label="Size", xalign=0))
        sp = Gtk.SpinButton.new_with_range(8, 400, 1)
        sp.set_value(self.cfg.d["font_size"])

        def size(s):
            self.cfg.d["font_size"] = int(s.get_value())
            self.cfg.save_soon()
            self.apply_params()
        sp.connect("value-changed", size)
        row.append(sp)
        bold = Gtk.ToggleButton()
        bl = Gtk.Label()
        bl.set_markup("<b>B</b>")
        bold.set_child(bl)
        bold.set_size_request(36, 36)
        bold.set_tooltip_text("Bold")
        bold.set_active(self.cfg.d["bold"])

        def tg(b):
            self.cfg.d["bold"] = b.get_active()
            self.cfg.save_soon()
            self.apply_params()
        bold.connect("toggled", tg)
        row.append(bold)
        return [dd, row]

    def popover_for(self, key):
        if key in self.popovers:
            return self.popovers[key]
        parent = self.tool_buttons["shape" if key.startswith("shape") else key]
        menu = key == "shape_menu"
        if key == "pen":
            kids = [self.slider_row("Size", "pen_size", 1, 40)]
        elif key == "hl":
            kids = [self.slider_row("Thickness", "hl_size", 4, 80),
                    self.slider_row("Opacity %", "hl_alpha", 10, 100)]
        elif key == "text":
            kids = self.font_rows()
        elif key == "shape":
            fill = Gtk.CheckButton(label="Fill shapes")
            fill.set_active(self.cfg.d["fill"])

            def fl(b):
                self.cfg.d["fill"] = b.get_active()
                self.cfg.save_soon()
                self.apply_params()
            fill.connect("toggled", fl)
            kids = [self.slider_row("Stroke", "shape_size", 1, 40), fill]
        else:  # shape_menu: the drop-down list of shapes
            kids = []
            for name, label in (("line", "Line   (L)"), ("rect", "Rectangle   (R)"),
                                ("ellipse", "Ellipse / circle   (E)"), ("tri", "Triangle   (G)")):
                row = Gtk.Box(spacing=10)
                row.append(icon_widget(name))
                row.append(Gtk.Label(label=label, xalign=0))
                b = Gtk.Button()
                b.add_css_class("flat")
                b.set_child(row)
                b.connect("clicked", lambda *_, n=name: (self.popovers["shape_menu"].popdown(),
                                                         self.choose_tool(n)))
                kids.append(b)
        pop = Gtk.Popover()
        pop.set_parent(parent)
        pop.set_position(Gtk.PositionType.BOTTOM)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2 if menu else 8)
        for side in ("start", "end", "top", "bottom"):
            getattr(box, f"set_margin_{side}")(4 if menu else 10)
        for k in kids:
            box.append(k)
        pop.set_child(box)
        pop.connect("closed", lambda *_: (self.canvas.tv or self.canvas).grab_focus())
        self.popovers[key] = pop
        return pop

    def show_options(self, key):
        self.choose_tool(self.cfg.d["shape"] if key == "shape" else key)
        self.popover_for(key).popup()

    # ---- tool / color / parameter state
    def choose_tool(self, name):
        c = self.canvas
        if name == "pick" and c.tool != "pick":
            self.prev_tool = c.tool
        if name in SHAPES:
            self.cfg.d["shape"] = name
            self.cfg.save_soon()
            self.tool_buttons["shape"].set_icon(name)
        c.set_tool(name)
        for key, b in self.tool_buttons.items():
            active = (key == name) or (key == "shape" and name in SHAPES)
            (b.add_css_class if active else b.remove_css_class)("tool-active")
        self.apply_color()
        self.apply_params()

    def finish_pick(self):
        self.choose_tool(self.prev_tool if self.prev_tool != "pick" else "pen")

    def cur_color_key(self):
        t = self.canvas.tool
        if t == "pick":
            t = self.prev_tool
        return "hl_rgb" if t == "hl" else "pen_rgb"

    def apply_color(self):
        rgb = parse_hex(self.cfg.d[self.cur_color_key()])
        c = self.canvas
        c.color = rgb
        self.palette.set_color(rgb)
        self.scroll_palette(self.palette.idx)
        self.color_btn.set_rgb(rgb)
        if c.text_active:
            c.place_text()
        c.queue_draw()

    def set_color(self, rgb):
        self.cfg.d[self.cur_color_key()] = hexcol(rgb)
        self.cfg.save_soon()
        self.apply_color()

    def custom_color(self):
        dlg = Gtk.ColorDialog()
        dlg.set_with_alpha(False)
        cur = Gdk.RGBA()
        cur.red, cur.green, cur.blue = self.canvas.color
        cur.alpha = 1.0

        def done(d, res):
            try:
                rgba = d.choose_rgba_finish(res)
            except GLib.Error:
                return
            self.set_color((rgba.red, rgba.green, rgba.blue))
        dlg.choose_rgba(self, cur, None, done)

    def apply_params(self):
        d, c = self.cfg.d, self.canvas
        t = c.tool
        c.size = d["shape_size"] if t in SHAPES else d["hl_size"] if t == "hl" else d["pen_size"]
        c.fill = d["fill"]
        c.hl_alpha = d["hl_alpha"] / 100
        c.font_family, c.font_size, c.bold = d["font_family"], int(d["font_size"]), d["bold"]
        if c.text_active:
            c.place_text()
        c.queue_draw()

    def scroll_palette(self, idx, center=False):
        if idx < 0:
            return
        adj = self.pal_sw.get_vadjustment()
        y, page = idx * Palette.H, adj.get_page_size()
        if center or y < adj.get_value() or y + Palette.H > adj.get_value() + page:
            adj.set_value(max(0, y - page / 2))

    def on_map(self, *_):
        if not self._centered:
            self._centered = True
            GLib.timeout_add(80, lambda: (self.scroll_palette(self.palette.idx, True), False)[1])

    def history_changed(self, can_undo, can_redo):
        self.undo_btn.set_sensitive(can_undo)
        self.redo_btn.set_sensitive(can_redo)

    def zoom_changed(self, s):
        self.zoom_btn.set_label(f"{s * self.dev * 100:.0f}%")

    def update_title(self):
        name = os.path.basename(self.path) if self.path else "Untitled"
        self.wtitle.set_title(("• " if self.canvas.dirty else "") + name)
        self.set_title(name)

    def on_canvas_size(self, w, h):
        """One-time correction so the image fits exactly if our chrome estimate was off."""
        if self._corrected:
            return
        self._corrected = True
        if self._capped:
            return
        nw, nh = math.ceil(self.canvas.w / self.dev), math.ceil(self.canvas.h / self.dev)
        dw, dh = (nw - w if w < nw else 0), (nh - h if h < nh else 0)
        if dw or dh:
            self.set_default_size(self._size[0] + dw, self._size[1] + dh)

    def toast(self, msg):
        self.toast_label.set_text(msg)
        self.toast_label.set_visible(True)
        if self._toast_id:
            GLib.source_remove(self._toast_id)
        self._toast_id = GLib.timeout_add(1300, self._hide_toast)

    def _hide_toast(self):
        self._toast_id = 0
        self.toast_label.set_visible(False)
        return False

    # ---- keyboard
    def on_key(self, _ctl, keyval, _code, state):
        c = self.canvas
        ctrl = bool(state & Gdk.ModifierType.CONTROL_MASK)
        shift = bool(state & Gdk.ModifierType.SHIFT_MASK)
        k = (Gdk.keyval_name(keyval) or "").lower()
        editing = c.text_active or isinstance(self.get_focus(), Gtk.Editable)
        if editing and (not ctrl or k in ("z", "y", "a", "c", "v", "x")):
            return False
        if ctrl:
            if k == "z":
                c.do_redo() if shift else c.do_undo()
            elif k == "y":
                c.do_redo()
            elif k == "s":
                self.save_as() if shift else self.save()
            elif k == "c":
                self.copy_image()
            elif k == "o":
                self.open_dialog()
            elif k == "n":
                self.app.capture(self)
            elif k in ("w", "q"):
                self.close()
            elif k == "0":
                c.zoom_fit()
            elif k == "1":
                c.zoom_actual()
            elif k in ("plus", "equal", "kp_add"):
                c.zoom_by(1.25)
            elif k in ("minus", "underscore", "kp_subtract"):
                c.zoom_by(1 / 1.25)
            elif k == "a":
                self.choose_tool("select")
                c.select_all()
            else:
                return False
            return True
        if k == "space":
            c.set_space(True)
            return True
        if k in ("delete", "backspace"):
            c.delete_selection()
            return True
        if k == "escape":
            c.escape()
            return True
        if k in KEYMAP and not shift:
            self.choose_tool(KEYMAP[k])
            return True
        return False

    def on_key_release(self, _ctl, keyval, _code, _state):
        if Gdk.keyval_name(keyval) == "space":
            self.canvas.set_space(False)

    # ---- file operations
    def copy_image(self):
        self.canvas.commit_text()
        surf = self.canvas.copy_surface()
        wl = shutil.which("wl-copy")
        if wl:
            buf = io.BytesIO()
            surf.write_to_png(buf)
            try:
                subprocess.run([wl, "-t", "image/png"], input=buf.getvalue(), check=True, timeout=5)
                self.toast("Copied")
                return
            except Exception:
                pass
        self.get_clipboard().set_texture(texture_from_surface(surf))
        self.toast("Copied")

    def write_png(self, path):
        self.canvas.surface.flush()
        tmp = path + ".tmp"
        self.canvas.surface.write_to_png(tmp)
        os.replace(tmp, path)

    def save(self, after=None):
        self.canvas.commit_text()
        path = self.path
        if not path:
            path = new_screenshot_path()
        elif not path.lower().endswith(".png"):
            path = os.path.splitext(path)[0] + "-annotated.png"
        try:
            self.write_png(path)
        except Exception as e:
            self.toast(f"Save failed: {e}")
            return False
        self.path = path
        self.canvas.set_dirty(False)
        self.toast("Saved")
        if after:
            after()
        return True

    def save_as(self):
        self.canvas.commit_text()
        dlg = Gtk.FileDialog()
        dlg.set_title("Save As")
        dlg.set_initial_name(os.path.basename(self.path) if self.path
                             else os.path.basename(new_screenshot_path()))
        dlg.set_initial_folder(Gio.File.new_for_path(
            os.path.dirname(self.path) if self.path else screenshots_dir()))

        def done(d, res):
            try:
                f = d.save_finish(res)
            except GLib.Error:
                return
            p = f.get_path()
            if not p.lower().endswith(".png"):
                p += ".png"
            try:
                self.write_png(p)
            except Exception as e:
                self.toast(f"Save failed: {e}")
                return
            self.path = p
            self.canvas.set_dirty(False)
            self.toast("Saved")
        dlg.save(self, None, done)

    def open_dialog(self):
        dlg = Gtk.FileDialog()
        flt = Gtk.FileFilter()
        flt.set_name("Images")
        flt.add_mime_type("image/*")
        dlg.set_default_filter(flt)
        dlg.set_initial_folder(Gio.File.new_for_path(screenshots_dir()))

        def done(d, res):
            try:
                f = d.open_finish(res)
            except GLib.Error:
                return
            self.app.open_path(f.get_path())
        dlg.open(self, None, done)

    def cleanup_popovers(self):
        for p in self.popovers.values():
            if p.get_parent():
                p.unparent()
        self.popovers.clear()

    def on_close(self, _w):
        self.canvas.commit_text()
        if not self.canvas.dirty:
            self.cfg.save()
            self.cleanup_popovers()
            return False
        d = Adw.AlertDialog(heading="Save changes?",
                            body="Your annotations haven't been saved.")
        d.add_response("cancel", "Cancel")
        d.add_response("discard", "Discard")
        d.add_response("save", "Save")
        d.set_response_appearance("discard", Adw.ResponseAppearance.DESTRUCTIVE)
        d.set_response_appearance("save", Adw.ResponseAppearance.SUGGESTED)
        d.set_default_response("save")
        d.set_close_response("cancel")

        def resp(_d, r):
            if r == "save":
                self.save(after=self.close)
            elif r == "discard":
                self.canvas.dirty = False
                self.close()
        d.connect("response", resp)
        d.present(self)
        return True


# --------------------------------------------------------------------- app
class App(Adw.Application):
    def __init__(self, stdin_data):
        super().__init__(application_id=APP_ID,
                         flags=Gio.ApplicationFlags.HANDLES_OPEN | Gio.ApplicationFlags.NON_UNIQUE)
        self.stdin_data = stdin_data
        self.settings = Settings()

    def do_startup(self):
        Adw.Application.do_startup(self)
        UI_CSS.load_from_string(
            ".tool-active { background: alpha(@accent_bg_color, 0.32); }"
            ".tool-active:hover { background: alpha(@accent_bg_color, 0.45); }"
            ".shotnote-toast { background: rgba(30, 30, 30, 0.88); color: white;"
            " padding: 6px 16px; border-radius: 999px; font-weight: 600; }")
        for prov in (TEXT_CSS, UI_CSS):
            Gtk.StyleContext.add_provider_for_display(
                Gdk.Display.get_default(), prov, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)

    def do_activate(self):
        if self.stdin_data:
            try:
                surf = surface_from_texture(Gdk.Texture.new_from_bytes(GLib.Bytes.new(self.stdin_data)))
            except GLib.Error as e:
                print("Could not read image from stdin:", e.message, file=sys.stderr)
                return
            Window(self, surf, None).present()
        else:
            self.capture()

    def do_open(self, files, _n, _hint):
        for f in files:
            self.open_path(f.get_path())

    def open_path(self, path):
        try:
            surf = surface_from_texture(Gdk.Texture.new_from_filename(path))
        except GLib.Error as e:
            print(f"Could not open {path}: {e.message}", file=sys.stderr)
            return
        Window(self, surf, path).present()

    # -- GNOME's screenshot UI through xdg-desktop-portal
    def capture(self, origin=None):
        self.hold()
        if origin:
            origin.set_visible(False)
        GLib.timeout_add(250 if origin else 1, self._start_capture, origin)

    def _start_capture(self, origin):
        def finish(path):
            if origin:
                origin.set_visible(True)
            if path:
                self.open_path(path)
            self.release()
        try:
            bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
            sender = bus.get_unique_name()[1:].replace(".", "_")
            token = f"shotnote{os.getpid()}x{int(time.time() * 1000)}"
            handle = f"/org/freedesktop/portal/desktop/request/{sender}/{token}"
            sub = []

            def on_response(_c, _s, _p, _i, _sig, params, *_):
                bus.signal_unsubscribe(sub[0])
                code, results = params.unpack()
                uri = results.get("uri") if code == 0 else None
                path = Gio.File.new_for_uri(uri).get_path() if uri else None
                finish(ensure_in_screenshots(path) if path else None)
            sub.append(bus.signal_subscribe(
                "org.freedesktop.portal.Desktop", "org.freedesktop.portal.Request", "Response",
                handle, None, Gio.DBusSignalFlags.NONE, on_response))

            def called(src, res):
                try:
                    src.call_finish(res)
                except GLib.Error as e:
                    print("Screenshot portal failed:", e.message, file=sys.stderr)
                    bus.signal_unsubscribe(sub[0])
                    finish(None)
            bus.call("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop",
                     "org.freedesktop.portal.Screenshot", "Screenshot",
                     GLib.Variant("(sa{sv})", ("", {"interactive": GLib.Variant("b", True),
                                                    "handle_token": GLib.Variant("s", token)})),
                     GLib.VariantType("(o)"), Gio.DBusCallFlags.NONE, -1, None, called)
        except GLib.Error as e:
            print("Cannot reach portal:", e.message, file=sys.stderr)
            finish(None)
        return False


def main():
    args, data = sys.argv[:], None
    if "-" in args[1:]:
        args.remove("-")
        data = sys.stdin.buffer.read()
    return App(data).run(args)


if __name__ == "__main__":
    sys.exit(main())
