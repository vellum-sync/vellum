function __vellum_preexec() {
    \command vellum store -- "$1"
}

\builtin typeset -ga preexec_functions
preexec_functions+=(__vellum_preexec)
