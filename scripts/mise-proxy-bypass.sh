#!/usr/bin/env bash

# The direct toolchain hosts return HTTP 407 through the configured proxy.
# Keep both conventional environment variable spellings in sync.
proxy_bypass_domains=(
  mise-versions.jdx.dev
  static.rust-lang.org
  sh.rustup.rs
  index.crates.io
)

append_no_proxy_domain() {
  local variable_name="$1"
  local current_value="${!variable_name-}"
  local domain

  for domain in "${proxy_bypass_domains[@]}"; do
    case ",$current_value," in
      *,"$domain",*) ;;
      *) current_value="${current_value:+$current_value,}$domain" ;;
    esac
  done

  printf -v "$variable_name" '%s' "$current_value"
  export "$variable_name"
}

configure_mise_no_proxy() {
  append_no_proxy_domain NO_PROXY
  append_no_proxy_domain no_proxy
}
