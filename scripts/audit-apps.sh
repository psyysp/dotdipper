#!/usr/bin/env bash
#
# Compare what the repository expects against what this machine actually has.
#
#   ./audit-apps.sh [path-to-install-apps.sh]
#
# Three outcomes per cask, because "not installed by brew" and "not installed"
# are different facts and only one of them is a problem:
#
#   via brew    brew knows about it
#   unmanaged   the app is on disk but brew did not put it there
#   absent      no app by that name anywhere
#
# The unmanaged check matches cask tokens against application bundle names,
# which is a heuristic: a cask whose app is named nothing like its token can
# still be reported absent. Treat that column as a prompt to look, not proof.
set -uo pipefail

SCRIPT="${1:-$HOME/install-apps.sh}"
if [[ ! -f "$SCRIPT" ]]; then
    echo "No $SCRIPT. Generate one with:" >&2
    echo "  dotdipper install apps-script -o ~/install-apps.sh" >&2
    exit 1
fi

# The generator single-quotes and escapes every entry, so the array
# definitions are data. Take them without running the install logic.
eval "$(sed -n '/^FORMULAE=(/,/^)/p; /^CASKS=(/,/^)/p; /^MAS_IDS=(/,/^)/p' "$SCRIPT")"

norm() { printf '%s' "${1:-}" | tr '[:upper:]' '[:lower:]' | tr -cd '[:alnum:]'; }

have_formulae=$(brew list --formula -1 2>/dev/null | sort -u)
have_casks=$(brew list --cask -1 2>/dev/null | sort -u)
have_mas=$(mas list 2>/dev/null | awk '{print $1}' | sort -u)

installed_apps=()
broken_links=()
for dir in /Applications /Applications/Utilities "$HOME/Applications"; do
    [[ -d "$dir" ]] || continue
    for app in "$dir"/*.app; do
        if [[ -e "$app" ]]; then
            installed_apps+=("$(norm "$(basename "$app" .app)")")
        elif [[ -L "$app" ]]; then
            # A bundle that is a symlink to nowhere looks installed in Finder
            # and satisfies nothing. Usually a leftover pointing into the
            # Intel Homebrew prefix on a machine that has since moved to
            # Apple silicon, and it will get in the way of reinstalling.
            broken_links+=("$app -> $(readlink "$app")")
        fi
    done
done

app_present() {
    # Tokens carry suffixes the bundle name drops: tailscale-app is Tailscale,
    # docker-desktop is Docker. Strip those, then allow the bundle name to
    # carry a version or edition the token does not: Dropzone 4, T3 Code (Alpha).
    local token stripped
    stripped=$(printf '%s' "$1" | sed -E 's/-(app|desktop|browser|cli|gui)$//')
    token=$(norm "$stripped")
    [[ -n "$token" ]] || return 1
    local candidate
    for candidate in ${installed_apps[@]+"${installed_apps[@]}"}; do
        [[ "$candidate" == "$token" || "$candidate" == "$token"* ]] && return 0
    done
    return 1
}

missing_formulae=()
for f in ${FORMULAE[@]+"${FORMULAE[@]}"}; do
    grep -qx "${f##*/}" <<<"$have_formulae" || missing_formulae+=("$f")
done

unmanaged=(); absent=()
for c in ${CASKS[@]+"${CASKS[@]}"}; do
    grep -qx "$c" <<<"$have_casks" && continue
    if app_present "$c"; then unmanaged+=("$c"); else absent+=("$c"); fi
done

missing_mas=()
for m in ${MAS_IDS[@]+"${MAS_IDS[@]}"}; do
    grep -qx "$m" <<<"$have_mas" || missing_mas+=("$m")
done

n_f=${#FORMULAE[@]}; n_c=${#CASKS[@]}; n_m=${#MAS_IDS[@]}
printf 'formulae : %d/%d installed\n' $(( n_f - ${#missing_formulae[@]} )) "$n_f"
printf 'casks    : %d/%d via brew, %d present but unmanaged, %d absent\n' \
    $(( n_c - ${#unmanaged[@]} - ${#absent[@]} )) "$n_c" "${#unmanaged[@]}" "${#absent[@]}"
if ! command -v mas >/dev/null 2>&1; then
    printf 'app store: mas is not installed, so %d title(s) went unchecked\n' "$n_m"
elif ! mas account >/dev/null 2>&1; then
    printf 'app store: not signed in, so %d title(s) went unchecked\n' "$n_m"
else
    printf 'app store: %d/%d installed\n' $(( n_m - ${#missing_mas[@]} )) "$n_m"
fi

if [[ ${#missing_formulae[@]} -gt 0 ]]; then
    printf '\nmissing formulae:\n'; printf '  %s\n' "${missing_formulae[@]}"
fi
if [[ ${#absent[@]} -gt 0 ]]; then
    printf '\nabsent casks:\n'; printf '  %s\n' "${absent[@]}"
    printf '\ninstall them with:\n  brew install --cask %s\n' "${absent[*]}"
fi
if [[ ${#unmanaged[@]} -gt 0 ]]; then
    printf '\npresent but installed outside brew (nothing to do unless you want brew to own them):\n'
    printf '  %s\n' "${unmanaged[@]}"
fi
if [[ ${#broken_links[@]} -gt 0 ]]; then
    printf '\nbroken application symlinks (look installed, are not):\n'
    printf '  %s\n' "${broken_links[@]}"
fi
if [[ ${#missing_mas[@]} -gt 0 ]] && command -v mas >/dev/null 2>&1 && mas account >/dev/null 2>&1; then
    printf '\nmissing App Store titles:\n'
    for m in "${missing_mas[@]}"; do printf '  %-12s %s\n' "$m" "$(mas info "$m" 2>/dev/null | head -1)"; done
fi
exit 0
