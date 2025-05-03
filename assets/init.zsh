if [[ "$(whence -w __fzfcmd)" != "__fzfcmd: function" ]]; then
    echo "fzf is required!" >&2
    echo "see https://github.com/junegunn/fzf" >&2
elif [[ -n "$__VELLUM_SETUP" || ! $- =~ i ]]; then
    true
else
    readonly __VELLUM_SETUP=1

    VELLUM_SESSION="$(vellum init session)"
    VELLUM_SESSION_START="$(vellum init timestamp)"
    export VELLUM_SESSION VELLUM_SESSION_START

    function __vellum_preexec() {
        \command vellum store -- "$1"
    }

    \builtin typeset -ga preexec_functions
    preexec_functions+=(__vellum_preexec)

    function __vellum_precmd() {
        __VELLUM_LINE=""
    }

    \builtin typeset -ga precmd_functions
    precmd_functions+=(__vellum_precmd)

    function vellum-search-widget() {
        local selected
        setopt localoptions noglobsubst noposixbuiltins pipefail no_aliases noglob nobash_rematch 2> /dev/null
        selected="$(vellum history --fzf ${VELLUM_HISTORY_ARGS[@]} |
        FZF_DEFAULT_OPTS=$(__fzf_defaults "" "-n2..,.. --scheme=history --bind=ctrl-r:toggle-sort --wrap-sign '\t↳ ' --highlight-line ${FZF_CTRL_R_OPTS-} --query=${(qqq)LBUFFER} +m --read0") \
        FZF_DEFAULT_OPTS_FILE='' $(__fzfcmd))"
        local ret=$?
        if [ -n "$selected" ]; then
            LBUFFER="${selected#*$'\t'}"
        fi
        zle reset-prompt
        return $ret
    }

    zle -N vellum-search-widget
    bindkey -M emacs '^R' vellum-search-widget

    function __vellum_previous() {
        local vellum_output
        local -a vellum_search
        if [[ -z "${__VELLUM_LINE}" ]]; then
            vellum_search+=("--prefix=${LBUFFER}")
        fi
        vellum_output="$(vellum move --with-id --session ${vellum_search[@]} ${VELLUM_MOVE_ARGS[@]} -- -1 ${__VELLUM_LINE})"
        __VELLUM_LINE="${vellum_output%%|*}"
        LBUFFER="${vellum_output#*|}"
    }

    function vellum-up() {
        if [[ "${LBUFFER}" == *$'\n'* ]]; then
            zle up-line
        else
            __vellum_previous
        fi
    }

    function __vellum_next() {
        local vellum_output
        local -a vellum_search
        if [[ -z "${__VELLUM_LINE}" ]]; then
            vellum_search+=("--prefix=${LBUFFER}")
        fi
        vellum_output="$(vellum move --with-id --session ${vellum_search[@]} ${VELLUM_MOVE_ARGS[@]} -- 1 ${__VELLUM_LINE})"
        __VELLUM_LINE="${vellum_output%%|*}"
        LBUFFER="${vellum_output#*|}"
    }

    function vellum-down() {
        if [[ "${RBUFFER}" == *$'\n'* ]]; then
            zle down-line
        else
            __vellum_next
        fi
    }

    zle -N up-line-or-history vellum-up
    zle -N down-line-or-history vellum-down
    zle -N history-substring-search-up __vellum_previous
    zle -N history-substring-search-down __vellum_next
fi
