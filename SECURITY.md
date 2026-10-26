# Security Policy

## Report a vulnerability

Please use [GitHub private vulnerability reporting](https://github.com/risingjerry/fusebox/security/advisories/new). Do not open a public issue containing a secret, credential, private path, or exploit detail.

## Secret handling contract

- Never commit `~/.config/fusebox/secrets.env` or copy it into a repository.
- The repository ignores `.env`, `*.env`, `secrets.env`, and `backups/`, but
  ignore rules are only a last line of defense; credential values belong only
  in the Fusebox private store.
- Never include secret values in issues, screenshots, logs, or diagnostic output.
- Use the macOS app or `fusebox secret set --stdin NAME` to write a value.
- Use `fusebox secret list` for audits; it returns names only.
- Declare credential names on the MCP and keep their values out of the MCP's
  ordinary `env` mapping. Generated wrappers grant only declared names.
- Pin package versions for MCPs launched through package runners such as `npx`.
- Rotate an affected provider credential immediately if exposure is suspected.

## Scope

Security reports should cover Fusebox itself: secret storage and transport, generated wrappers, MCP config projection, instruction management, the on-demand MCP hub, and the native macOS app.

Third-party MCP servers, CLI tools, Skill sources, and providers keep their own security boundaries.
An MCP process can read the credentials explicitly granted to it; do not grant a
credential to code whose source or package version you do not trust.
