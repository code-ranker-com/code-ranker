# CLA bot: how it works and where signatures live

Maintainer notes for the Contributor License Agreement automation. The
contributor-facing summary is in [CONTRIBUTING.md](../CONTRIBUTING.md); the
agreement text itself is [CLA.md](../CLA.md).

## Why

While the project is licensed under Apache-2.0, the CLA preserves the owner's
right to relicense later (copyleft, source-available, or commercial dual
licensing) without chasing every past contributor for consent. Contributors
keep the copyright in their work but grant a perpetual, irrevocable license
that includes relicensing (CLA.md §2).

## Setup

Everything runs inside GitHub — no external service:

- **Workflow:** [`.github/workflows/cla.yml`](../.github/workflows/cla.yml),
  using [`contributor-assistant/github-action`](https://github.com/contributor-assistant/github-action),
  pinned to the v2.6.1 release commit
  (`ca4a40a7d1004f18d9960b404b97e5f30a505a08`).
- **Allowlist** (no signature required): `ffedoroff`, `dependabot[bot]`,
  `github-actions[bot]`.
- **Branch protection:** the `CLAAssistant` status check is required on
  `main`, so a PR cannot be merged until the CLA is signed.

## The flow on a pull request

1. An external contributor opens a PR.
2. The bot comments with a link to `CLA.md` and sets a failing
   `CLAAssistant` status check.
3. The contributor replies in the PR with exactly:
   `I have read the CLA Document and I hereby sign the CLA`
4. The bot records the signature, the check turns green, and the PR can be
   merged. Commenting `recheck` re-runs the check manually.
5. The signature is remembered — the same contributor is never asked again.

## Where the list of signers is stored

In this repository, on a dedicated branch:

- **Branch:** `cla-signatures` (created by the bot on the first signature;
  it does not exist until then).
- **File:** `signatures/version1/cla.json` —
  <https://github.com/code-ranker-com/code-ranker/blob/cla-signatures/signatures/version1/cla.json>

The file is a JSON array with one record per signer: GitHub login, user id,
the PR number where they signed, and the signing timestamp. Commits are made
by `github-actions[bot]` (this is why the workflow has `contents: write`).

Because signatures live in git:

- they are backed up with every clone of the repository;
- the history of the file is an audit trail of who signed what and when.

**Do not edit, rebase, or delete the `cla-signatures` branch.** It is the
project's legal archive. Exclude it from any branch-cleanup automation.

## Changing the CLA text

The `version1` path segment is the version of the agreement text. If
`CLA.md` ever changes in substance:

1. Update `CLA.md` on `main`.
2. Change `path-to-signatures` in `.github/workflows/cla.yml` to
   `signatures/version2/cla.json`.

The bot will then ask everyone — including past signers — to sign the new
version on their next PR. Keep the old `version1` file in place: it remains
the proof of agreement for contributions merged under the old text.

## Caveats

- The `CLAAssistant` required check only appears in the branch-protection
  check list after the workflow has run at least once.
- The workflow triggers on `pull_request_target`, so it runs the workflow
  definition from `main`, not from the PR branch — contributors cannot
  bypass the check by editing the workflow in their PR.
- Corporate contributors (contributing as part of their job) should have
  their employer sign a Corporate CLA — see the note at the end of
  [CLA.md](../CLA.md).
