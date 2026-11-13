# Branches: from work to `main`

How a change travels from an agent's work branch to a release. Until 0.1.0 is out the old way holds: checked work may be pushed to `main`. [The day we switch](#the-day-we-switch) lists what changes then.

## The flow after the switch

```
claude/…  fix/…  feature/…      work branches: changes are made here
        |
        v   (merge or push, once the checks pass)
     staging                    checked work; CI runs; installers are built
        |
        v   (pull request "staging -> main", merged by the owner only)
       main                     only what the owner has hand-tested
        |
        v   (tag, see releasing.md)
     v0.2.0                     a release
```

- Every push to `staging` runs CI and builds the installers for every OS (`.github/workflows/staging-build.yml`). They are kept as downloadable build artifacts, not as a GitHub release, so the owner can hand-test exactly what would ship.
- `main` changes only when the owner merges the pull request "staging -> main".
- A quick fix on a release: branch from `main`, open a pull request to `main`, the owner merges it, then `main` is merged back into `staging`, so the fix is not lost.

## What goes where

| Branch | What lands there | Who puts it there | CI | Installers |
|---|---|---|---|---|
| `claude/…`, `fix/…`, `feature/…` | Work in progress | Agents and contributors | Yes, on every push | No |
| `staging` | Finished work whose checks pass | Agents and contributors (merge or push) | Yes, must pass | Built on every push, kept as artifacts |
| `main` | What the owner has hand-tested | The owner, by merging "staging -> main" (or a quick-fix pull request) | Yes, must pass | Built from a tag only ([releasing.md](releasing.md)) |

## The owner's routine

All of it is on the GitHub website, in the repository.

1. **Find the staging build.** Open the **Actions** tab. Click the newest **Staging build** run with a green tick. Wait if it is still running (the installers take a while).
2. **Download it.** At the bottom of the run's page, under **Artifacts**, click the one for your computer (`ambrotype-linux-x64`, `ambrotype-windows-x64`, `ambrotype-macos-arm64`, and so on). It downloads as a zip; unzip it and install the file inside.
3. **Try it.** Open a photo, edit, undo, export. Try what [WHATS-NEXT.md](WHATS-NEXT.md) says changed since last time. Note what is wrong.
4. **If it is good:** open the **Pull requests** tab, then **New pull request**. Set base `main` and compare `staging`. Create it and read the list of commits. Click **Merge pull request** and confirm. If a pull request from staging to main is already open, use that one.
5. **If it is not good:** don't merge. Tell an agent what you saw; the fix goes to `staging` and the build runs again. Start again at step 1.
6. **To release:** when `main` holds what you want to ship, tag it as [releasing.md](releasing.md) says. The tag builds the installers and makes a draft release to check and publish.
7. **After a quick fix on `main`:** open a pull request from `main` to `staging` and merge it, so `staging` has the fix too.

## The day we switch

Do this once, after 0.1.0 is out.

1. Create `staging` from `main`: on the **Code** tab, open the branch menu, type `staging`, and choose **Create branch: staging from main**.
2. Set the rules. **The settings are yours to click: nothing in the repository can set them.** Go to **Settings -> Branches** (or **Settings -> Rules -> Rulesets**) and add:
   - For `main`: require a pull request before merging; restrict who can push to you alone; require the CI status checks to pass (the `test` and `dependency licences` jobs); block force pushes; block deletion.
   - For `staging`: require the CI status checks to pass; block force pushes; block deletion.
3. Check the **Staging build** workflow runs once: **Actions -> Staging build -> Run workflow**, choose `staging`.
4. Change `AGENTS.md`: the paragraph under "Before you commit" says to push to `staging`, never `main`, instead of "as now". Change `CONTRIBUTING.md` and the pull request template the same way: pull requests go to `staging`.
5. Tell agents in their first message of the day if they have a chat open from before: old chats still think `main` is the place.
