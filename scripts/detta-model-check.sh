#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tla_tools_version="1.7.4"
tla_tools_url="https://github.com/tlaplus/tlaplus/releases/download/v${tla_tools_version}/tla2tools.jar"

if [[ -n "${TLA2TOOLS_JAR:-}" ]]; then
  tla_tools_jar="$TLA2TOOLS_JAR"
else
  tool_cache="${DETTA_TOOL_CACHE:-$repo_root/target/tools}"
  mkdir -p "$tool_cache"
  tla_tools_jar="$tool_cache/tla2tools-${tla_tools_version}.jar"
  if [[ ! -s "$tla_tools_jar" ]]; then
    curl -fsSL "$tla_tools_url" -o "$tla_tools_jar.tmp"
    mv "$tla_tools_jar.tmp" "$tla_tools_jar"
  fi
fi

java -cp "$tla_tools_jar" tlc2.TLC \
  -deadlock \
  -simulate num=32 \
  -depth 20 \
  -seed 1 \
  -workers auto \
  -config "$repo_root/models/DeTTaBlockExecution.cfg" \
  "$repo_root/models/DeTTaBlockExecution.tla"
