# Resources macOS

`iperf3` (binaire universal x86_64+arm64) est compilé depuis les sources
d'[esnet/iperf](https://github.com/esnet/iperf) par le job GitHub Actions
`build-macos` puis déposé ici avant l'étape `tauri build`. Le binaire est
embarqué dans `.app/Contents/Resources/resources/macos/iperf3`.

`speedtest` (Ookla CLI) reste téléchargé au runtime depuis l'app ou pris
via Homebrew — pas (encore) bundlé côté macOS.

Le binaire n'est pas commit dans le repo (`.gitignore`).
