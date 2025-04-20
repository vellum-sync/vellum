[ -n "$__VELLUM_SETUP" ] && return || readonly __VELLUM_SETUP=1

VELLUM_SESSION="$(vellum init session)"
VELLUM_SESSION_START="$(vellum init timestamp)"
export VELLUM_SESSION VELLUM_SESSION_START

function __vellum_preexec() {
    \command vellum store -- "$1"
}

\builtin typeset -ga preexec_functions
preexec_functions+=(__vellum_preexec)
