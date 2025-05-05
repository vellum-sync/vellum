# vellum

## Introduction

vellum syncs shell command history between hosts using a git repository as a
central synchronisation point.

## Installation

Pre-built packages and binaries are available on the [Releases] page. Or it can
be built from source using cargo.

The compiled binary is standalone, and can generate shell integration and man
pages.

## Setup

### Dependencies

Currently vellum depends on [fzf](https://github.com/junegunn/fzf), and for
bash, [bash-preexec](https://github.com/rcaloras/bash-preexec) is also required.
So these tools will need to be installed before vellum can be used.

### Encryption Key

Once you have vellum, and the pre-requisites installed, then you need to
generate an encryption key. This key should be kept private, and treated as a
password. The sync data also cannot be recovered if it is lost, so a record
should be kept somewhere safe.

To generate an encryption key run:

```shell
vellum init key
```

### Create sync repo

You will need to create a git repo to provide a sync-point between machines. It
is strongly recommended that this be a private repo (even though the stored data
is encrypted).

### Configuration file

The configuration file is written in TOML, and lives in
`~/.config/vellum/config.toml` by default (you can set VELLUM_CONFIG to use an
alternate location).

The default values are intended to be usable as reasonable values, but the
details of how to connect to your git sync repo need to be supplied.

A minimal configuration probably looks something like:

```toml
[sync]
url = "git@github.com:vellum-sync/vellum-history.git"
ssh_key = "/home/vellum/.ssh/github.key"
```

Where `url` should be set to the clone URL of your repo, and ssh_key should be
set to the ssh private key to be used, if you are using an SSH URL.

### Shell integration

Once you have your key, and config file setup, then you can integrate vellum
into your shell setup, for bash:

```bash
export VELLUM_KEY="g5PpAVupuiN4OQ+MVByDdggVVjaepp1sRI7cXZE4d60="

source /path/to/bash-preexec.sh
eval "$(fzf --bash)"
VELLUM_MOVE_ARGS+=("--no-duplicates")
eval "$(vellum init bash)"
eval "$(vellum complete bash)"
```

or zsh:

```zsh
export VELLUM_KEY="g5PpAVupuiN4OQ+MVByDdggVVjaepp1sRI7cXZE4d60="

eval "$(fzf --zsh)"
VELLUM_MOVE_ARGS+=("--no-duplicates")
eval "$(vellum init zsh)"
eval "$(vellum complete zsh)"
```

**NOTE**: The key above is included as an example, and should not be used.

(if you install from a package, then the shell completion can also be installed
using the normal shell completion setup)

### Import existing history

Once you setup vellum then your history will start empty. You can import history
from your existing shell history using the `import` command. This can read from
stdin, or a file, a list of commands to import into the history.

For example:

```shell
vellum import -f $HISTFILE
```

## Interacting with your history

Once the shell integration is setup, then all commands typed will be stored by
vellum in a local server, which will be automatically started. This server will
then sync the history with the git repo in the background (or when you run
`vellum sync`).

Vellum uses the concept of a "session", which denotes a single shell session,
and tracks which session commands were stored from. Then when using the up and
down movement vellum will only show commands which were stored by the current
session, or before the current session started (this provides a stable history
list whilst you are scrolling). For the Ctrl-R integration the complete history
across all sessions is shown by default (though you can set
`VELLUM_HISTORY_ARGS+=("--session")` to make it only show the current session).

In addition to the shell integration the `vellum history` command can be used to
view and search the history. This is similar to the `history` or `fc` commands
used to query shell history. See `vellum history --help` for more details.

## Editing your history

By default vellum records all commands that are run, and persists them in the
sync repo. However, sometimes you might accidentally type something you didn't
mean to into your shell, or decide that you don't want to see something you
typed deliberately again in your history. Vellum provides some commands to help
with these issues.

In order to use the history modification commands it is important to know about
entry IDs. Every command that is stored in vellum has a unique ID that is
assigned when it is stored. The IDs for commands can be seen by running `vellum
history --id` or `vellum history --verbose`. When editing history, the IDs are
used to track which entry is being modified.

The first command for editing history is `vellum delete` which takes one or more
entry IDs and marks those history entries as deleted. It does not remove the
command from the sync repo, but it will never be shown in history again.

The second command for editing history is `vellum edit`, which takes a number of
optional filters (see `vellum edit --help` for details) and then presents the
matching commands as a list in an editor (set `VELLUM_EDITOR` if you want to
control which editor is used) with the ID and command one per line. Once this
file has been modified then any changes to commands will be saved, and any
removed entries will be marked as deleted. As with the `vellum delete` command,
these changes are recorded as changes and the original commands will still be
stored in the sync repo.

The final history editing command is `vellum rebuild`. This command does not
make changes to the history itself, but rather rebuilds the sync repo so that
the commit history is flattened so that only a new commit with the current state
exists. Thus removing any edited or deleted commands from the sync repo (though
the old data will still persist until purged by git, and any tags or branches
made by the user will not be touched, only the default branch).
