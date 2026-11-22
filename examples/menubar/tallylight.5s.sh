#!/bin/sh
# tallylight in the macOS menu bar, for SwiftBar or xbar (the "5s" in the file name is the refresh rate).
# Reads the daemon of this machine: no pairing, no library. Needs curl and jq (both ship with macOS 15).
# <xbar.title>tallylight</xbar.title>
# <xbar.desc>Shows which coding agents are working and which one is waiting for you.</xbar.desc>
# <xbar.dependencies>tallylight,jq</xbar.dependencies>
PORT="${TALLYLIGHT_PORT:-47700}"

if ! snapshot="$(curl -fsS -m 2 "http://127.0.0.1:$PORT/sessions" 2>/dev/null)"; then
  echo "○ | color=gray"
  echo "---"
  echo "tallylight is not running"
  exit 0
fi

# Titre : la lampe rouge et le nombre d'agents qui attendent, sinon le nombre d'agents au travail
printf "%s\n" "$snapshot" | jq -r '
  (.sessions | map(select(.state == "input-required")) | length) as $waiting
  | (.sessions | map(select(.state == "working")) | length) as $working
  | if $waiting > 0 then "● \($waiting) | color=red"
    elif $working > 0 then "◐ \($working)"
    else "○" end'
echo "---"
printf "%s\n" "$snapshot" | jq -r '
  def name: .title // (.cwd | split("/") | last);
  def mark: if .state == "input-required" then "●" elif .state == "working" then "◐"
            elif .state == "failed" then "◆" else "○" end;
  if (.sessions | length) == 0 then "no agent session"
  else .sessions | sort_by(.state != "input-required", name) | .[]
    | "\(mark) \(name)  \(.state // "idle")"
      + (if .state == "input-required" then " | color=red" else "" end)
      + (if .detail then "\n-- \(.detail)" else "" end)
  end'
