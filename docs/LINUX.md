# Linux development candidate

The initial target is **Ubuntu 24.04 x86_64**, booted with systemd, active
systemd-resolved and `/etc/resolv.conf` linked to its stub resolver. A desktop
Secret Service with an unlocked, password-protected persistent keyring is required.
GNOME and XFCE are acceptance targets. Debian and other desktop/init/DNS combinations
are not supported until separately tested. See STATUS.md for actual results;
configured CI and a compiling binary are not passing VPN acceptance.

The `.deb` installs the ordinary-user GUI, pinned Xray/sing-box engines and a
root-owned systemd service. Install using `sudo apt install ./Moth-<version>-linux-x64.deb`.
Open Moth Desktop from the application menu. GUI elevation is not required.
Installation starts the service but does not connect a VPN. Before upgrading,
disconnect VPN and exit the GUI. An active connection, unresolved private-network
lease or running GUI blocks package replacement. Remove using `sudo apt remove moth-desktop`.
User keyring profiles remain intact, including after `apt purge`; package scripts
never traverse user homes or delete keyring items. The package manager owns program
files; there is no AppImage or direct replacement of dpkg-owned files by an updater.
Signed in-app updates remain separate unfinished work.

The GUI uses a fixed Unix socket with kernel peer authentication; connections are
owned by the authenticated UID. The service accepts root maintenance and normal
desktop UIDs with a private login runtime directory. It exposes only the versioned
typed command set, not arbitrary commands, executable paths or raw core JSON.
The root-owned installation has no symlinks or group/other write access; every core
launch rechecks its compiled-in SHA-256 pin. Profile secrets remain in the user's
Secret Service, outside the system daemon and package directories.

The candidate uses `moth0`, routing table/priority 20220, rule protocol 242 and
upstream socket mark 0x4d4f5448. Existing use of these identifiers is a conflict,
not permission to overwrite another network configuration. Before changing routes,
the daemon writes a protected network journal. It adds two exact policy rules and
private-link routes, and configures only the private link's resolved DNS. Teardown
and systemd ExecStopPost remove only rules matching that journal; there is no rule
table flush or physical DNS/resolv.conf rewrite. Deleting the private link removes
its routes and resolver state. Engines receive parent-death SIGKILL and belong to
the service cgroup. Conflicting recovery state is preserved and reported rather
than deleting unknown rules.

Window close hides only when a real StatusNotifier host is present. Without one,
the window minimizes to the taskbar; disappearance of a host restores a hidden
window. The GUI Exit button and tray Exit use the same cancellation/disconnection
barrier and require an authenticated stop confirmation. Missing IPC alone is not
proof that a tunnel stopped. Other-user tunnels are not disconnected by GUI Exit.

There is no persistent kill switch or automatic reconnect. Failure returns ordinary
networking; complete leak protection, concurrent VPN compatibility and every routing
domain/desktop variation are not claimed. These limits also apply to the candidate
until real TUN/HTTPS/DNS/IPv4/IPv6/crash acceptance completes.

Build prerequisites follow the official [Tauri Linux requirements](https://v2.tauri.app/start/prerequisites/).
Fetch pinned engines, run `npm ci`, then `bash scripts/build-linux.sh <new-artifact-directory>`.
This builds a path-remapped release and uses Tauri's standard Debian packager.
The disposable CI scripts exercise real keyring persistence, UID ownership,
TCP/UDP IPv4/IPv6, HTTPS/DNS, engine/daemon death, native GUI with/without XFCE tray,
and actual apt install/remove/reinstall/purge. See STATUS.md before treating any
candidate artifact as a verified user release.
