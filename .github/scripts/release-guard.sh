#!/usr/bin/env bash
# The release guard (ci-release design D5, D6). Runs before anything is built.
#
#   release-guard.sh pins <workflow.yml>...  every action from outside this repo
#                                            is pinned to a full commit SHA
#   release-guard.sh tag <tag>               the tag matches the repo's version,
#                                            is checked out, and is on main
#   release-guard.sh version app|server      print the repo's version
#   release-guard.sh environment <name>      the environment is protected (needs
#                                            GH_TOKEN, GITHUB_REPOSITORY)
#   release-guard.sh environment-json <name> <reviewer> <env.json> <policies.json>
#
# Run from inside the repository. MAIN_REF (default origin/main) names main.
set -euo pipefail

die() { echo "release-guard: $*" >&2; exit 1; }

repo="$(git rev-parse --show-toplevel)"
main_ref="${MAIN_REF:-origin/main}"

app_version() {
  sed -n 's/^version:[[:space:]]*//p' "$repo/app/pubspec.yaml" | head -1 | tr -d '[:space:]'
}

server_version() {
  # The first `version = "..."` of server/Cargo.toml is its [package] version.
  sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$repo/server/Cargo.toml" | head -1
}

check_pins() {
  [ $# -gt 0 ] || die "pins: name at least one workflow"
  local bad="" file n line ref
  for file in "$@"; do
    [ -f "$file" ] || die "pins: no such file: $file"
    n=0
    while IFS= read -r line; do
      n=$((n + 1))
      [[ "$line" =~ ^[[:space:]]*(-[[:space:]]+)?uses:[[:space:]]*([^[:space:]]+)(.*)$ ]] || continue
      ref="${BASH_REMATCH[2]}"
      ref="${ref%\"}"; ref="${ref#\"}"; ref="${ref%\'}"; ref="${ref#\'}"
      case "$ref" in ./*) continue ;; esac
      if ! [[ "$ref" =~ @[0-9a-f]{40}$ ]] || ! [[ "$line" =~ \#[[:space:]]*v[0-9] ]]; then
        bad="$bad"$'\n'"  $file:$n: $ref"
      fi
    done <"$file"
  done
  [ -z "$bad" ] || die "actions not pinned to a full commit SHA with a '# v<version>' comment:$bad"
  echo "pins: every action is pinned to a commit SHA"
}

check_tag() {
  local tag="${1:-}" version want build last t n commit
  [ -n "$tag" ] || die "tag: no tag given"
  git rev-parse -q --verify "refs/tags/$tag" >/dev/null || die "tag $tag does not exist here"
  commit="$(git rev-parse "refs/tags/$tag^{commit}")"
  [ "$(git rev-parse HEAD)" = "$commit" ] || die "HEAD is not the commit of $tag"
  git rev-parse -q --verify "$main_ref^{commit}" >/dev/null || die "no $main_ref (fetch all history)"
  git merge-base --is-ancestor "$commit" "$main_ref" || die "$tag ($commit) is not on $main_ref"

  case "$tag" in
    app-v*)
      version="${tag#app-v}"
      [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+\+[0-9]+$ ]] || die "$tag is not app-v<name>+<build>"
      want="$(app_version)"
      [ "$version" = "$want" ] || die "$tag does not match app/pubspec.yaml version $want"
      build="${version##*+}"
      last=0
      while IFS= read -r t; do
        [ -n "$t" ] && [ "$t" != "$tag" ] || continue
        n="${t##*+}"
        [[ "$n" =~ ^[0-9]+$ ]] && [ "$n" -gt "$last" ] && last="$n"
      done < <(git tag -l 'app-v*')
      [ "$build" -gt "$last" ] || die "build $build is not above the last released build $last"
      echo "tag: $tag matches pubspec, build $build > $last, on $main_ref"
      ;;
    server-v*)
      version="${tag#server-v}"
      [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "$tag is not server-v<major>.<minor>.<patch>"
      want="$(server_version)"
      [ "$version" = "$want" ] || die "$tag does not match server/Cargo.toml version $want"
      echo "tag: $tag matches server/Cargo.toml, on $main_ref"
      ;;
    *) die "$tag is neither app-v* nor server-v*" ;;
  esac
}

# GitHub creates a missing environment, unprotected, the first time a job
# names it. So every job in `release` first checks that the environment really
# has the owner as required reviewer, no admin bypass, and only the release tag
# rules (design D3). The two JSON documents are GitHub's answers for
# environments/<env> and environments/<env>/deployment-branch-policies.
check_environment_json() {
  local env="$1" reviewer="$2" env_json="$3" policies_json="$4" problems
  problems="$(jq -rn --arg reviewer "$reviewer" \
    --slurpfile e "$env_json" --slurpfile p "$policies_json" '
    ($e[0]) as $env | ($p[0]) as $pol |
    [
      (if ([$env.protection_rules[]? | select(.type == "required_reviewers")
            | .reviewers[]? | .reviewer.login] | index($reviewer)) == null
       then "no required reviewer \($reviewer)" else empty end),
      (if $env.can_admins_bypass != false then "administrators can bypass it" else empty end),
      (if $env.deployment_branch_policy.custom_branch_policies != true
       then "not limited to selected tags" else empty end),
      (if ([$pol.branch_policies[]? | "\(.type):\(.name)"] | sort)
          != ["tag:app-v*", "tag:server-v*"]
       then "deployment rules are not exactly tags app-v* and server-v*" else empty end)
    ] | .[]')" || die "environment $env: cannot read GitHub's answer"
  [ -z "$problems" ] || die "environment $env is not protected: $(echo "$problems" | paste -sd ';' -)"
  echo "environment: $env needs $reviewer, no admin bypass, tags app-v* and server-v* only"
}

check_environment() {
  local env="${1:-}" reviewer="${RELEASE_REVIEWER:-koppathachan}" work
  [ -n "$env" ] && [ -n "${GITHUB_REPOSITORY:-}" ] || die "environment: needs a name and GITHUB_REPOSITORY"
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand now: the folder of this call
  trap "rm -rf '$work'" RETURN
  gh api "repos/$GITHUB_REPOSITORY/environments/$env" >"$work/env.json" ||
    die "environment $env: not found"
  gh api "repos/$GITHUB_REPOSITORY/environments/$env/deployment-branch-policies" >"$work/policies.json" ||
    die "environment $env: cannot read its deployment rules"
  check_environment_json "$env" "$reviewer" "$work/env.json" "$work/policies.json"
}

case "${1:-}" in
  pins) shift; check_pins "$@" ;;
  environment) shift; check_environment "$@" ;;
  environment-json) shift; check_environment_json "$@" ;;
  tag) shift; check_tag "$@" ;;
  version)
    case "${2:-}" in
      app) app_version ;;
      server) server_version ;;
      *) die "version: app or server" ;;
    esac
    ;;
  *) sed -n '2,13p' "$0" >&2; exit 2 ;;
esac
