#!/usr/bin/env bash
# Usage: ./install.sh [--default-png]
set -euo pipefail
cd "$(dirname "$0")"

sudo dnf install -y python3-gobject gtk4 libadwaita python3-cairo \
    xdg-desktop-portal-gnome wl-clipboard

install -Dm755 shotnote.py "$HOME/.local/bin/shotnote"
install -Dm644 dev.local.Shotnote.desktop "$HOME/.local/share/applications/dev.local.Shotnote.desktop"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true

# Global shortcut: Super+Shift+S -> shotnote
KEY=org.gnome.settings-daemon.plugins.media-keys
P=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/shotnote/
cur=$(gsettings get $KEY custom-keybindings)
if [[ "$cur" != *"$P"* ]]; then
  if [[ "$cur" == "@as []" || "$cur" == "[]" ]]; then new="['$P']"; else new="${cur%]}, '$P']"; fi
  gsettings set $KEY custom-keybindings "$new"
fi
gsettings set $KEY.custom-keybinding:$P name 'Shotnote'
gsettings set $KEY.custom-keybinding:$P command "$HOME/.local/bin/shotnote"
gsettings set $KEY.custom-keybinding:$P binding '<Shift><Super>s'

if [[ "${1:-}" == "--default-png" ]]; then
  xdg-mime default dev.local.Shotnote.desktop image/png
  echo "Shotnote is now the default PNG opener (undo: xdg-mime default org.gnome.Loupe.desktop image/png)"
fi
echo "Done. Press Super+Shift+S, or run: shotnote"
