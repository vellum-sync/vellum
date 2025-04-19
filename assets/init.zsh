[ -n "$__VELLUM_SETUP" ] && return || readonly __VELLUM_SETUP=1

VELLUM_SESSION="$(vellum init session)"
export VELLUM_SESSION

function __vellum_preexec() {
    \command vellum store -- "$1"
}

\builtin typeset -ga preexec_functions
preexec_functions+=(__vellum_preexec)
