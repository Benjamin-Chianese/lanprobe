# Resources Windows

`speedtest.exe` (Ookla CLI) est téléchargé automatiquement par `build.rs` lors
des builds ciblant Windows et embarqué dans l'installeur NSIS aux côtés de
`LanProbe.exe`. Le binaire n'est pas commit dans le repo (`.gitignore`) — il
est récupéré une seule fois puis mis en cache dans le volume `target-windows`
du runner.

URL source : `https://install.speedtest.net/app/cli/ookla-speedtest-1.2.0-win64.zip`

À l'exécution, `network/speedtest.rs::bundled_speedtest_path` cherche
`resources/win/speedtest.exe` à côté de l'exécutable de l'app.
