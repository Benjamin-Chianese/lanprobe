# Resources Linux

`iperf3` (binaire statique x86_64) est compilé depuis les sources
d'[esnet/iperf](https://github.com/esnet/iperf) par le job GitHub Actions
`build-linux` puis déposé ici avant l'étape `tauri build`. Il est embarqué
dans le `.deb` (sous `/usr/lib/lanprobe/resources/linux/iperf3`) et dans
l'AppImage (à côté de l'exécutable).

Le binaire n'est pas commit dans le repo (`.gitignore`).
