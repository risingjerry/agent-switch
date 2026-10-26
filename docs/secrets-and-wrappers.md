# Secrets And Wrappers

Fusebox keeps secrets out of generated app configs by using wrapper commands.

Native app configs and CC Switch rows contain:

- wrapper command path;
- non-secret args.

They do not contain:

- API keys;
- tokens;
- bearer values;
- per-tool secret values.

## Secrets File

Default path:

```bash
~/.config/fusebox/secrets.env
```

Example shape (names vary with the MCP registry):

```bash
FILESYSTEM_TOKEN=...
SEARCH_API_KEY=...
```

The status output reports missing secret names only. It never prints values.

Use the CLI to add or update secrets:

```bash
secret-producing-command | fusebox secret set --stdin FIRECRAWL_API_KEY
fusebox secret set --fd 3 FIRECRAWL_API_KEY 3< <(secret-producing-command)
fusebox secret get --fd 3 FIRECRAWL_API_KEY 3> >(secret-consuming-command)
fusebox secret list
fusebox secret delete FIRECRAWL_API_KEY
```

`--stdin` and `--fd` keep the value out of the Fusebox process arguments.
Replace `secret-producing-command` with a program that emits the value; never
replace that placeholder with the value itself. Positional secret values are
rejected so they cannot enter process arguments or shell history.

Secret input must be non-empty, single-line UTF-8 no larger than 64 KiB. The
CLI removes one final LF or CRLF from a producer. `--stdin` rejects interactive
TTY input, and `--fd` accepts inherited read descriptors numbered 3 or higher.

`secret get` never returns a value through stdout, stderr, argv, or the
environment. It writes the exact UTF-8 value only to the inherited descriptor,
which must be numbered 3 or higher and must not be a TTY or an alias of stdout
or stderr.

`secret list` prints names only. It does not print values.

`secret delete` removes one named value under the same exclusive lock and atomic
write contract used by `secret set`. It preserves comments, unrelated names,
and mode `0600`, and never prints a secret value.

## macOS Secret UI

The macOS app uses the same backend contracts rather than reading the file
directly:

- writes stream the value to `secret set --stdin`;
- the eye button reveals or hides a value directly without a system authentication dialog;
- revealed bytes travel through a private mode-`0600` FIFO connected to
  `secret get --fd 3`, never stdout or stderr;
- revealed values stay visible until the eye button is clicked again or the
  user leaves the secret page;
- copy is a normal explicit clipboard action.

## Global Instructions

`reconcile` also maintains global runtime instructions so new Codex, Claude Code, and Hermes sessions know where secrets and MCP configuration belong.

Managed files:

```text
~/.config/fusebox/instructions/AGENTS.md
~/.config/fusebox/instructions/CLAUDE.md
~/.config/fusebox/instructions/HERMES.md
```

Native app integration:

- Codex: `~/.codex/config.toml` points `model_instructions_file` at the Fusebox `AGENTS.md`.
- Claude Code: `~/.claude/CLAUDE.md` receives a managed Fusebox block.
- Hermes: `~/.hermes/SOUL.md` receives a managed Fusebox block.

These instructions are policy only. The source of truth remains `secrets.env`, wrapper scripts, and generated native MCP config.

Run `fusebox agents --json` to inspect detection, instruction enrollment,
and synchronization status for the currently supported targets: Codex, Claude
Code, and Hermes. A future agent needs its own adapter before Fusebox can
manage it safely.

## Wrapper Behavior

Each wrapper:

1. parses the secrets file as data; it never shell-sources the file;
2. removes inherited sensitive environment variables;
3. validates the secret names declared by that MCP;
4. injects only those declared values into the child environment;
5. expands declared `${SECRET_NAME}` placeholders in command arguments without
   writing the value into the wrapper or native app config;
6. prints missing names only;
7. executes the configured MCP command.

During native stdio import, credential-shaped environment values and values
following flags such as `--api-key` or `--token` are moved into the private
store. Their generated argument placeholder is kept in central config. Direct
configuration rejects recognized credential-shaped literals in commands,
arguments, or ordinary environment values.

Wrappers are deterministic so `reconcile` can skip unchanged writes.
