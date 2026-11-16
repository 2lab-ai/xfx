#!/usr/bin/env bash
#
# The static half of the audit: every function of xfx's that reads the
# process environment, and every call of one, at a revision.
#
#   audit-static.sh <rev>     (run in the repository)
#
# Reads code only (`git grep` at <rev>), never the environment itself.
set -euo pipefail

main() {
  local rev=$1
  echo "# audit-static at $(git rev-parse --short=8 "$rev")"
  echo
  echo "## every std::env read in src (comment lines excluded)"
  git grep -nE '(^|[^[:alnum:]_])env::(var|var_os|vars|vars_os|home_dir)\(|getenv\(' "$rev" -- 'src/*.rs' 'src/**/*.rs' \
    | sed "s/^$rev://" | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true
  echo
  echo "## the readers routed through process_env! (head only)"
  git grep -n 'process_env!(' "$rev" -- 'src/*.rs' 'src/**/*.rs' | sed "s/^$rev://" \
    | grep -v '^src/lib.rs' | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true
  echo
  for reader in 'Environment::from_process()' 'Endpoint::from_process()' 'GitEnvironment::from_process()' \
                'LinkBase::from_environment()' 'RuntimeConfig::load(' 'Runtime::new(' 'McpRuntime::new(' \
                'catalog_for(' 'credits::question(' 'usage_question(' 'title_asker(' 'should_run(' 'now_ms()'; do
    echo "## callers of $reader"
    git grep -nF "$reader" "$rev" -- 'src/*.rs' 'src/**/*.rs' | sed "s/^$rev://" \
      | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true
    echo
  done
  echo "## integration harnesses: how each builds a child's environment"
  git grep -nE 'Command::new|env_clear|clear_env\(|env_remove\(|CONTROLLED_VARS' "$rev" -- 'tests/*.rs' 'tests/support/*.rs' \
    | sed "s/^$rev://" | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true
}

main "$@"
