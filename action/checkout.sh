#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'got action error: %s\n' "$*" >&2
  exit 1
}

warn() {
  printf '::warning::%s\n' "$*" >&2
}

validate_repo_path() {
  local value="$1"
  [[ -n "$value" && "$value" != /* && "$value" != */ && "$value" != *\\* && "$value" != *$'\n'* && "$value" != *$'\r'* ]] || return 1
  local -a components
  IFS='/' read -r -a components <<< "$value"
  local component
  local folded_component
  for component in "${components[@]}"; do
    folded_component="$(printf '%s' "$component" | tr '[:upper:]' '[:lower:]')"
    [[ -n "$component" && "$component" != "." && "$component" != ".." && "$folded_component" != ".git" ]] || return 1
  done
}

[[ -n "${GITHUB_WORKSPACE:-}" ]] || fail "GITHUB_WORKSPACE is not set"
[[ -n "${GOT_REPOSITORY:-}" ]] || fail "repository could not be resolved"
[[ "$GOT_REPOSITORY" != -* && "$GOT_REPOSITORY" == */* ]] || fail "repository must be in owner/repository form"
sparse_path="${GOT_PATH:-}"
if [[ -n "$sparse_path" ]]; then
  validate_repo_path "$sparse_path" || fail "path must be a repository-relative directory"
fi

fetch_depth="${GOT_FETCH_DEPTH:-1}"
[[ "$fetch_depth" =~ ^[1-9][0-9]*$ ]] || fail "fetch-depth must be a positive integer"

workspace="$(cd "$GITHUB_WORKSPACE" && pwd -P)"
directory="${GOT_DIRECTORY:-.}"
[[ "$directory" != /* && "$directory" != *\\* && "$directory" != *$'\n'* && "$directory" != *$'\r'* ]] || fail "directory must be relative to GITHUB_WORKSPACE and must not contain newlines"
if [[ "$directory" == "." ]]; then
  checkout_dir="$workspace"
else
  IFS='/' read -r -a directory_components <<< "$directory"
  for component in "${directory_components[@]}"; do
    [[ -n "$component" && "$component" != "." && "$component" != ".." ]] || fail "directory must not contain empty, '.' or '..' components"
  done
  checkout_dir="$workspace/$directory"
fi

mkdir -p "$checkout_dir"
shopt -s dotglob nullglob
existing_entries=("$checkout_dir"/*)
if (( ${#existing_entries[@]} > 0 )); then
  fail "checkout destination '$checkout_dir' is not empty"
fi

server="${GOT_SERVER_URL:-https://github.com}"
server="${server%/}"
remote="$server/$GOT_REPOSITORY.git"
auth_config=""
if [[ -n "${GOT_TOKEN:-}" ]]; then
  encoded_token="$(printf 'x-access-token:%s' "$GOT_TOKEN" | base64 | tr -d '\r\n')"
  auth_config="http.${server}/.extraheader=AUTHORIZATION: basic ${encoded_token}"
fi

git_remote() {
  if [[ -n "$auth_config" ]]; then
    git -c "$auth_config" "$@"
  else
    git "$@"
  fi
}

git_checkout() {
  if [[ -n "$auth_config" ]]; then
    git -c "$auth_config" -C "$checkout_dir" "$@"
  else
    git -C "$checkout_dir" "$@"
  fi
}

unset_config_if_set() {
  local key="$1"
  local status
  if git_checkout config --get "$key" >/dev/null 2>&1; then
    git_checkout config --unset "$key" || fail "could not disable partial-clone setting '$key'"
  else
    status=$?
    [[ "$status" -eq 1 ]] || fail "could not inspect partial-clone setting '$key'"
  fi
}

ref="${GOT_REF:-}"
if [[ -z "$ref" && "$GOT_REPOSITORY" == "${GITHUB_REPOSITORY:-}" ]]; then
  ref="${GITHUB_SHA:-}"
fi
if [[ -z "$ref" ]]; then
  head_info="$(git_remote ls-remote --symref "$remote" HEAD 2>&1)" || {
    fail "could not resolve the repository default branch"
  }
  ref="$(printf '%s\n' "$head_info" | sed -n 's/^ref: \(refs\/heads\/[^[:space:]]*\)[[:space:]]*HEAD$/\1/p')"
  [[ -n "$ref" ]] || fail "repository did not advertise a default branch; set the ref input"
fi

git init --quiet "$checkout_dir" || fail "could not initialize checkout directory"
git_checkout remote add origin "$remote" || fail "could not configure checkout remote"
paths=()
if [[ -n "$sparse_path" ]]; then
  paths+=("$sparse_path")
fi
while IFS= read -r additional_path; do
  [[ -z "$additional_path" ]] && continue
  validate_repo_path "$additional_path" || fail "additional path '$additional_path' must be repository-relative"
  paths+=("$additional_path")
done <<< "${GOT_ADDITIONAL_PATHS:-}"
if (( ${#paths[@]} > 0 )); then
  git_checkout sparse-checkout init --cone || fail "sparse checkout requires a recent version of Git"
  git_checkout sparse-checkout set -- "${paths[@]}" || fail "could not configure sparse paths"
fi

git_checkout config extensions.partialClone origin
git_checkout config remote.origin.promisor true
git_checkout config remote.origin.partialclonefilter blob:none

filter_enabled=true
if fetch_output="$(git_checkout fetch --quiet --depth="$fetch_depth" --filter=blob:none origin "$ref" 2>&1)"; then
  if grep -qiE 'filter(ing)? (is )?not (recognized|supported)|does not support.*filter' <<< "$fetch_output"; then
    warn "remote ignored blob filtering; unselected blobs may be transferred"
    filter_enabled=false
  fi
else
  if grep -qiE 'filter.*(not supported|unsupported|not recognized|does not allow)|does not support.*filter' <<< "$fetch_output"; then
    warn "remote rejected blob filtering; retrying with shallow sparse checkout"
    filter_enabled=false
    unset_config_if_set extensions.partialClone
    unset_config_if_set remote.origin.promisor
    unset_config_if_set remote.origin.partialclonefilter
    fetch_output="$(git_checkout fetch --quiet --depth="$fetch_depth" origin "$ref" 2>&1)" || {
      printf '%s\n' "$fetch_output" >&2
      fail "could not fetch repository ref"
    }
  else
    printf '%s\n' "$fetch_output" >&2
    fail "could not fetch repository ref"
  fi
fi

if [[ "$filter_enabled" == false ]]; then
  unset_config_if_set extensions.partialClone
  unset_config_if_set remote.origin.promisor
  unset_config_if_set remote.origin.partialclonefilter
fi

if [[ -n "$sparse_path" ]]; then
  path_entry="$(git_checkout ls-tree -d --name-only FETCH_HEAD -- ":(literal)$sparse_path")" || fail "could not inspect selected path"
  [[ -n "$path_entry" ]] || fail "path does not exist as a directory at the selected ref"
fi
git_checkout checkout --quiet --detach FETCH_HEAD || fail "could not check out the requested ref"
sha="$(git_checkout rev-parse HEAD)"
absolute_directory="$(cd "$checkout_dir" && pwd -P)"
[[ "$absolute_directory" != *$'\n'* && "$absolute_directory" != *$'\r'* ]] || fail "checkout directory must not contain newlines"
{
  printf 'sha=%s\n' "$sha"
  printf 'directory=%s\n' "$absolute_directory"
} >> "$GITHUB_OUTPUT"

printf 'Checkout complete at %s\n' "$sha"
