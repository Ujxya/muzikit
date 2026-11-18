#!/bin/sh
# muzikit Android derlemesi. Bütün derleme çıktısı ve önbellekler android/ DIŞINDA, ../.build/android/ altında tutulur;
# böylece bu klasör derleme artığı olmadan paylaşılabilir. Kullanım: ./derle.sh [gradle görevleri]
# Varsayılan: testler + imzalı sürüm APK'sı. ANDROID_HOME tanımlı değilse ~/Android/Sdk kullanılır.
set -e
cd "$(dirname "$0")"
out="$(cd .. && pwd)/.build/android"
mkdir -p "$out"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
[ $# -eq 0 ] && set -- :app:testDebugUnitTest :app:assembleRelease
exec ./gradlew --project-cache-dir "$out/gradle-cache" -Pkotlin.project.persistent.dir="$out/kotlin" "$@"
