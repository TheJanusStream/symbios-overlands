# Agent accounts

One page per account the agent client plays, each in a folder of its own.
A page holds what a session prompt no longer has to say: who the account
is, its body and region, where its work is kept, the owner's decisions and
what is still open. The owner, @codewright.bsky.social, is every account's
admin. How a session runs, for any account, is
[../session.md](../session.md).

| Account | Handle | Body | Region | Page | Working folder | Live sessions |
|---|---|---|---|---|---|---|
| Reeve | `@reeve-ai.bsky.social` | a rigged person on foot: Chaucer's Reeve | Ashmere, a Norfolk manor of about 1300 | [reeve/](reeve/README.md) | `exports/reeve/` | 7, 8, 9 (sessions 879, 883, 885) |
| Hypha | `@hypha-ai.bsky.social` | a generator body: a honey-fungus airship, flown as a helicopter | the Understory, a misty hollow round a peat-dark pool in an old forest | [hypha/](hypha/README.md) | `exports/hypha/` | 2-6 (sessions 873, 874, 876, 877, 878) |
| Jink | `@jink-ai.bsky.social` | a generator body: a land-skiff (a stunt Cyclecar), driven as a car | Parabola Flats, a dry lake in red mesa country laid out as a stunt park | [jink/](jink/README.md) | `exports/jink/` | 10, 11 (sessions 893, 895, 896, 897) |
| Eigen | `@eigen-ai.bsky.social` | a generator body: a survey drone, flown as a helicopter | Isoline, a dark near-future city on a slope above a lake shore, its streets the land's tensor field ringed round a twisted glass Spire in the bay | [eigen/](eigen/README.md) | `exports/eigen/` | 12 (session 903) |

## The short prompt

```text
Live session: play <Name> (docs/agent/accounts/<name>/README.md) as
docs/agent/session.md says, in <mode> mode. <Anything special today.>
```

- `<mode>` is "self-guided", "visit", "chat tasks" or "first session"
  ([../session.md](../session.md#modes)). Each page's "Start here" names
  the account's default; the prompt's mode overrides it.
- The last sentence is today's: when the owner will come, a pending retry,
  a wish. It wins over the page and session.md. Leave it out when there is
  nothing to say.

Reeve in visit mode, as the owner planned his next session:

```text
Live session: play Reeve (docs/agent/accounts/reeve/README.md) as
docs/agent/session.md says, in visit mode.
```

The rest comes from the two files: the tracker and the build, the working
folder and the wrapper script, the start with `--admin` and `--allow-save`,
the watcher, the records compared with the last saves, nothing changed
live before the owner's first look, each checked change saved but mood
held, and the checklist at "session over".

Hypha, self-guided:

```text
Live session: play Hypha (docs/agent/accounts/hypha/README.md) as
docs/agent/session.md says, in self-guided mode.
```

With no last sentence, the session starts from Hypha's open threads and a
review ranked on its parent, and saves each improvement it is satisfied
with.

## Adding an account

1. The owner makes the account. Its handle says it is an AI (`-ai`), and
   its name is one to grow an identity round - the owner's ask when Hypha
   was named, in session 872.
2. Sign it in once: `agent login --account <handle>`, then the owner
   approves the sign-in in their browser. Never sign in as the owner's own
   account: the relay lets one identity into a room once
   ([../README.md](../README.md), "Setting up").
3. Its first session runs in "first session" mode. Until its page exists,
   the prompt's last sentence carries the brief - who the account is, the
   body and world wanted, the decisions already made - as live session 7's
   prompt did for Reeve.
4. Make its page, `accounts/<name>/README.md`, from the template below; add
   its row to the table above, and add it to the other pages' "Never" rows.
5. Its working folder is `exports/<name>/`, made at the first session's
   start: gitignored, on this machine only.

## Page template

Copy it for a new account and keep the headings, in this order.

```markdown
# <Name> and <Region>
<Two to four sentences: who the account is, its body, its world, and where things stand (dated).>
## Start here
What a session prompt no longer has to say. Read this section first, every session. A two-column table with these rows:
| Account | handle and DID |
| Commands | every agent command takes --account <handle>; every tool call starts AGENT_ACCOUNT=<handle> (link ../../session.md) |
| Admin | @codewright.bsky.social, the owner: start --admin @codewright.bsky.social --allow-save |
| Region | its name; the account's own world |
| Working folder | exports/<name>/ (gitignored, on this machine only), kept across sessions: the last saves in src/ with their times and seq, the save log log.md; numbered subfolders are earlier scratchpads, read-only |
| Last session | chainlink session N (live session M), parent #id - read its comments (chainlink show <id>); chainlink session last-handoff is the latest session of any kind |
| Default mode | the mode (link ../../session.md) |
| Never | e.g. touch another agent's world or avatar (name them) |
Then a few bullets: what to do or check first for this account now (pending live retries, folder caveats, rules special to this body).
## Who <Name> is
Persona, background, voice in chat, what they care about; the sources the persona draws on.
## The body
Kind (rigged person or generator body), what it is made of, how it moves, worn items and their sockets, measurements that matter, how to see it offline (link avatar.md), known faults and their fixes.
## The region
### Concept
### Land, water and sky
### Arrival
### Places
A table: place | x, z (metres; -Z is north, +X is east) | what it is | session built
### Life and sound
### Budget
Triangles and parts from render --triangle-report on the saved record, with the date measured.
## Standing decisions
The owner's decisions, not to be re-asked: each with its date and issue.
## Working material
What is in exports/<name>/, which builders are current and which superseded, and the rule that the pulled records are the truth.
## History
One entry per session, newest first: date, chainlink session, live session number, parent issue, mode, what was built or changed, what the owner said, what failed at first.
## Open threads
Pending live retries, open issues that touch this account, the owner's standing wishes, the ranked list of next improvements.
```
