#!/usr/bin/env bash
# Checks the workflow files against release-automation (ci-release design D3,
# D6, D9, D11). Read-only.
#
#   release-workflows-check.sh [<workflows dir>]   default .github/workflows
#
# Every workflow:     no pull_request_target; no ssh, scp or deploy/pi/deploy.sh.
# release-*.yml       (except release-lint.yml):
#   - triggers only `push: tags` and `workflow_dispatch`
#   - top-level `permissions: {}`
#   - a job that uses a secret (other than GITHUB_TOKEN) or asks for a write
#     permission names `environment: release` and runs only on a tag push
#   - a job without the environment uses no secret and asks for no write
#   - no cache (actions/cache, rust-cache, flutter-action cache: true)
#   - every checkout has persist-credentials: false
#   - every action is pinned (release-guard.sh pins)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dir="${1:-.github/workflows}"
[ -d "$dir" ] || { echo "no such folder: $dir" >&2; exit 2; }

status=0
ruby -ryaml - "$dir" <<'RUBY' || status=1
dir = ARGV[0]
problems = []
Dir.glob(File.join(dir, "*.{yml,yaml}")).sort.each do |path|
  name = File.basename(path)
  text = File.read(path)
  doc = YAML.safe_load(text, aliases: false) || {}
  # YAML 1.1: a bare `on:` key loads as true.
  on = doc.key?("on") ? doc["on"] : doc[true]
  triggers = case on
             when String then [on]
             when Array then on
             when Hash then on.keys
             else []
             end
  problems << "#{name}: uses pull_request_target" if triggers.include?("pull_request_target")
  (doc["jobs"] || {}).each do |job_id, job|
    (job["steps"] || []).each do |step|
      run = step["run"].to_s
      if run =~ /(^|[\s;&|(])(ssh|scp)\s/ || run =~ /(^|[\s;&|(])(\.\/)?deploy\/pi\/deploy\.sh/
        problems << "#{name}: job #{job_id} reaches the Pi or relay (ssh/scp/deploy.sh)"
      end
    end
  end

  next unless name.start_with?("release-") && name != "release-lint.yml"

  extra = triggers - ["push", "workflow_dispatch"]
  problems << "#{name}: triggers other than push tags / workflow_dispatch: #{extra.join(', ')}" unless extra.empty?
  if on.is_a?(Hash) && on["push"]
    push = on["push"] || {}
    unless push.is_a?(Hash) && push.keys == ["tags"]
      problems << "#{name}: push trigger must have tags only"
    end
  end
  problems << "#{name}: top-level permissions must be {}" unless doc["permissions"] == {}

  (doc["jobs"] || {}).each do |job_id, job|
    job_text = YAML.dump(job)
    env = job["environment"]
    env = env["name"] if env.is_a?(Hash)
    perms = job["permissions"]
    perms = {} unless perms.is_a?(Hash)
    writes = perms.select { |_, v| v == "write" }.keys
    secrets = job_text.scan(/secrets\.([A-Za-z0-9_]+)/).flatten.uniq - ["GITHUB_TOKEN"]
    if job["permissions"].nil? || job["permissions"].is_a?(String)
      problems << "#{name}: job #{job_id} must list its permissions"
    end
    if env == "release"
      cond = job["if"].to_s
      unless cond.include?("github.event_name == 'push'") && cond.include?("startsWith(github.ref, 'refs/tags/")
        problems << "#{name}: job #{job_id} (environment release) must run only on a tag push"
      end
    else
      problems << "#{name}: job #{job_id} names environment #{env.inspect}" unless env.nil?
      problems << "#{name}: job #{job_id} uses secrets #{secrets.join(', ')} without environment release" unless secrets.empty?
      problems << "#{name}: job #{job_id} asks for #{writes.join(', ')}: write without environment release" unless writes.empty?
    end
    (job["steps"] || []).each do |step|
      uses = step["uses"].to_s
      with = step["with"] || {}
      if uses.start_with?("actions/cache") || uses.include?("rust-cache") || (uses.include?("flutter-action") && with["cache"] != false)
        problems << "#{name}: job #{job_id} uses a cache (#{uses})"
      end
      if uses.start_with?("actions/checkout@") && with["persist-credentials"] != false
        problems << "#{name}: job #{job_id} checkout lacks persist-credentials: false"
      end
    end
  end
end
problems.each { |p| puts "FAIL  #{p}" }
exit(problems.empty? ? 0 : 1)
RUBY

shopt -s nullglob
release_files=()
for f in "$dir"/release-*.yml; do
  [ "$(basename "$f")" = release-lint.yml ] || release_files+=("$f")
done
if [ "${#release_files[@]}" -gt 0 ]; then
  "$here/release-guard.sh" pins "${release_files[@]}" || status=1
fi

[ "$status" = 0 ] && echo "workflows: release-automation checks pass"
exit "$status"
