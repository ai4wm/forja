# End-to-End Scenarios

## Scenario 1: First-Time Provider Setup

**Command/Action**
`forja setup`

**Precondition**
The binary is installed or the project is run from source. The user can interact with the terminal.

**Expect**
The setup wizard lists available providers, lets the user register credentials or OAuth access, prompts for a default model, and saves assistant and user labels.

**Verify**
Confirm that `~/.forja/config.toml` is created or updated and that the selected provider/model becomes the active default on the next run.

**Status**
Implemented in code.

## Scenario 2: Provider Login

**Command/Action**
`forja login openai`
or
`forja login gemini`
or
`forja login anthropic`

**Precondition**
The user has a browser available for OAuth flows or an API token for manual entry.

**Expect**
The login flow stores provider credentials in `~/.forja/auth.json` and prints a success or failure result in the terminal.

**Verify**
Confirm that the provider token exists in `auth.json` and that later startup can resolve the active provider without asking again.

**Status**
Implemented in code.

## Scenario 3: Start an Interactive Session

**Command/Action**
`forja`

**Precondition**
The user has completed setup or provides overrides such as `--provider` and `--model`.

**Expect**
The runtime loads config, bootstrap identity, tools, and memory, then enters the streaming CLI loop and accepts natural-language input.

**Verify**
Confirm that the banner is printed, the active provider/model is shown, and the prompt accepts user input.

**Status**
Implemented in code.

## Scenario 4: Switch Runtime Model

**Command/Action**
`/models`
then
`/model <number-or-name>`

**Precondition**
The session is running and at least one provider is configured.

**Expect**
The assistant lists configured models and switches the active provider/model without restarting the process.

**Verify**
Confirm that `/model` reports the new active selection after the switch succeeds.

**Status**
Implemented in code.

## Scenario 5: Use Vision Input

**Command/Action**
`/ss [prompt]`
or
`/image <path> [prompt]`

**Precondition**
Vision is enabled and the environment provides a usable screen capture or image file path.

**Expect**
The runtime captures the screen or loads the image, sends it through the vision analyzer, and returns a text result to the session.

**Verify**
Confirm that the assistant prints a vision response or a clear failure message if capture or analysis cannot run.

**Status**
Implemented in code.

## Scenario 6: Queue and Inspect Autonomous Work

**Command/Action**
`/task <description>`
then optionally
`/skills`
or
`/unresolved`

**Precondition**
The session is running with access to the local audit database.

**Expect**
The runtime stores task or autonomy state in `audit.db` and exposes it through slash commands and the dashboard.

**Verify**
Confirm that the command returns structured task or autonomy information and that related rows appear in dashboard-backed tables.

**Status**
Implemented in code.

## Scenario 7: Open the Local Dashboard

**Command/Action**
`/dashboard`

**Precondition**
The session is running and the configured local port is available.

**Expect**
The runtime starts an Axum server on localhost, opens the browser, and serves the audit, debates, budget, and task views.

**Verify**
Confirm that `http://localhost:3700/` or the configured dashboard port loads the dashboard and that `/api/tasks` returns JSON.

**Status**
Implemented in code.

## Scenario 8: Run Debate Mode

**Command/Action**
`/debate <topic>`

**Precondition**
The session is running with a working provider and the creation engine is enabled by the current binary.

**Expect**
The runtime executes a structured debate flow and returns a synthesized result while logging the transcript for later inspection.

**Verify**
Confirm that debate transcript entries appear in the session output and that dashboard debate endpoints expose the saved transcript.

**Status**
Implemented in code.
