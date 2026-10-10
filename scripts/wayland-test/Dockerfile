# A headless GNOME Wayland session for testing Heyo on the desktop that
# current Ubuntu ships (26.04: GNOME 50, no X11 session), without touching
# the developer's own session. See ./run.sh.
FROM ubuntu:26.04

ENV DEBIAN_FRONTEND=noninteractive LANG=C.UTF-8

RUN apt-get update && apt-get install -y --no-install-recommends \
      # The session.
      gnome-shell gnome-session-bin gnome-settings-daemon dbus dbus-user-session dbus-x11 \
      # Stands in for systemd-logind, which GNOME Shell will not start without.
      python3-dbusmock \
      xdg-desktop-portal xdg-desktop-portal-gnome xdg-desktop-portal-gtk \
      libgl1-mesa-dri mesa-utils libegl1 \
      # What Heyo links against, and the Node runtime for extensions.
      libwebkit2gtk-4.1-0 libgtk-3-0t64 libayatana-appindicator3-1 nodejs \
      # Tools Heyo shells out to, and ones the tests use.
      libnotify-bin xdg-utils xdg-user-dirs curl ca-certificates \
      wl-clipboard gnome-screenshot python3 python3-gi gir1.2-gtk-3.0 \
      fonts-dejavu-core fonts-noto-cjk ibus ibus-libpinyin \
    && rm -rf /var/lib/apt/lists/*

# An ordinary user, as on a desktop.
RUN useradd --create-home --uid 1001 --shell /bin/bash tester \
    && mkdir -p /home/tester/.local/share/gnome-shell/extensions && chown -R tester:tester /home/tester/.local
COPY --chown=tester:tester harness-extension /home/tester/.local/share/gnome-shell/extensions/heyo-test-harness@heyo.local
COPY session.sh /usr/local/bin/session.sh
RUN chmod +x /usr/local/bin/session.sh
WORKDIR /home/tester
ENTRYPOINT ["/usr/local/bin/session.sh"]
