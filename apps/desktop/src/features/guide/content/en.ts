/** Phase 6.7: Full Guide — English. Keep the same structure as `vi.ts`. `**x**` = bold. */
import type { GuideTopic } from "./types";

export const guideEn: GuideTopic[] = [
  {
    id: "start",
    title: "Getting started & the main screen",
    summary: "What the app does and what each part of the window is for.",
    sections: [
      {
        id: "what",
        title: "What is this app?",
        blocks: [
          {
            kind: "p",
            text: "NANOMultipleGitGUI lets you use Git with buttons instead of typed commands, and is built for people who have several GitHub / GitLab accounts (for example a work account and a personal one). Each repository remembers which account it uses, so commits and pushes always go out under the right name.",
          },
          {
            kind: "term",
            term: "Git",
            text: "a tool that keeps every saved version of your project, so you can go back in time and work with other people without overwriting each other.",
          },
          {
            kind: "term",
            term: "Repository (repo)",
            text: "one project folder that Git keeps the history of.",
          },
          {
            kind: "tip",
            text: "The first time you open the app, the short **Getting started** wizard checks that Git is installed and helps you add accounts. You can run it again from **Settings → Getting started**.",
          },
        ],
      },
      {
        id: "areas",
        title: "The parts of the window",
        blocks: [
          {
            kind: "list",
            items: [
              "**Top bar**: the account pill (which account this repo uses — the thin line at the very top of the window takes that account's color), one tab per open repository, the **+** button (open, create or clone a repository) and **Search / commands** (Ctrl+K).",
              "**Toolbar**: Fetch, Pull, Push, Branch, Stash, Pop, Undo, Redo; on the right **My work**, Sidebar, Panel, Terminal and **Settings**. When the window is narrow, extra buttons move into the **⋯** menu.",
              "**Left panel**: your branches (**Local** = on this computer, **Remote** = on the server), Tags, Stashes, PRs / MRs, Issues, LFS, Submodules, Worktrees. The box at the top filters branches by name.",
              "**Graph** (middle): every commit as a row, newest at the top, with lines showing how branches split and join. Columns: Branch / Tag, Graph, Commit message, Author, Date. A **WIP** row at the top means you have changes that are not committed yet.",
              "**Right panel**: **Changes** (files you edited, the commit box) — or **Commit details** when you click a commit in the graph.",
              "**Status bar** (bottom): the current branch, how many commits are ahead (↑) / behind (↓) the server, and the transfer queue (running fetch / pull / push / downloads).",
            ],
          },
          {
            kind: "p",
            text: "Messages appear in a bar under the top bar. Many of them have buttons that fix the problem for you — read them before closing. A **yellow bar** means an operation (merge, rebase…) is paused and waiting for you.",
          },
          {
            kind: "tip",
            text: "Drag the thin borders between panels to resize them (double-click a border to reset). Ctrl+B hides / shows the left panel, Ctrl+Alt+B the right panel.",
          },
        ],
      },
      {
        id: "first-day",
        title: "A normal day in 5 steps",
        blocks: [
          {
            kind: "steps",
            items: [
              "Open your repository (tab at the top) and click **Pull** to get your teammates' latest work.",
              "Double-click the branch you want to work on in the left panel (**Local**), or create one with **Branch**.",
              "Edit files in your code editor. They appear in the right panel under **Unstaged**.",
              "Click **Stage** on the files you want to save, write a short summary, then click **Commit**.",
              "Click **Push** to send your commits to the server.",
            ],
          },
        ],
      },
    ],
  },
  {
    id: "repos",
    title: "Open, create or clone a repository",
    summary:
      "Bring a project into the app: from this computer, brand new, or from GitHub / GitLab.",
    sections: [
      {
        id: "open",
        title: "Open a repository on this computer",
        blocks: [
          {
            kind: "steps",
            items: [
              "Click **+** at the right of the tabs (top bar), then **Open repository…**.",
              "Choose the project folder. If it is not a Git repository yet, the app offers to create one there.",
            ],
          },
          {
            kind: "p",
            text: "Open repositories come back as tabs the next time you start the app. **+** also lists recently opened repositories.",
          },
        ],
      },
      {
        id: "new",
        title: "Create a new repository",
        blocks: [
          {
            kind: "steps",
            items: [
              "Click **+** → **New repository…** and choose (or create) an empty folder.",
              "Keep the initial branch name (usually main) and click **Create repository**.",
              "The **New project** strip under the toolbar walks you through the rest: pick files in the Changes panel, write a message and click **Commit**.",
              "Click **Publish** (the Push button becomes Publish while the repository has no remote): choose the account, where to create it (your account or an organization / group), the name, Private / Public, then **Create and push**. The app creates an empty repository on GitHub / GitLab, adds it as remote origin bound to that account and pushes your branch.",
              "Already created the repository on the web? Use the **Use existing repository (URL)** tab and paste its URL. A new branch that is not on the server yet shows a **Publish branch** button.",
            ],
          },
        ],
      },
      {
        id: "clone",
        title: "Clone a repository from GitHub / GitLab",
        blocks: [
          {
            kind: "term",
            term: "Clone",
            text: "download a full copy of a project from the server to your computer, history included.",
          },
          {
            kind: "steps",
            items: [
              "Click **+** → **Clone repository…**.",
              "Search the list (it shows the repositories of every signed-in account) or paste a URL in **Or paste URL**. The app guesses the right account; change it under **Account** if needed.",
              "Choose the **Folder**. Under **Options** you can also download submodules, make a shallow copy or choose which LFS files to download.",
              "Click **Clone**. Progress shows in the dialog; you can cancel at any time (the half-downloaded folder is removed).",
            ],
          },
          {
            kind: "tip",
            text: "Want your own copy of someone else's project? In the clone dialog, tick **Fork to … then clone**: the app makes the fork on your account, clones it and adds the original as the “upstream” remote.",
          },
        ],
      },
    ],
  },
  {
    id: "accounts",
    title: "Accounts",
    summary:
      "Several GitHub / GitLab accounts, which account a repo uses, switching, SSH keys, signing in again.",
    sections: [
      {
        id: "add",
        title: "Add an account",
        blocks: [
          {
            kind: "steps",
            items: [
              "Open **Settings** (gear on the toolbar) → **Manage accounts…** → tab **Accounts** → **Add account**.",
              "Choose where the account lives: GitHub.com, GitHub Enterprise, GitLab.com or a self-hosted GitLab.",
              "Choose how to sign in: a short device code typed on the website, signing in with the browser, or pasting a personal access token (PAT).",
            ],
          },
          {
            kind: "p",
            text: "Already signed in with the GitHub CLI (gh) or GitLab CLI (glab)? The top of the account manager lists those accounts — click **Add** and they are ready without signing in again. Tokens are stored in your system's password store (keychain), never in plain files.",
          },
          {
            kind: "term",
            term: "Token",
            text: "a long secret code that acts like a password for the app; it can be revoked on the website at any time.",
          },
        ],
      },
      {
        id: "which",
        title: "Which account does a repository use?",
        blocks: [
          {
            kind: "p",
            text: "The **account pill** at the left of the top bar shows it, with the reason: chosen manually, locked, since clone, or by a rule. If a repository has no account yet, a banner asks you to choose one.",
          },
          {
            kind: "list",
            items: [
              "**Rules** (account manager → tab **Rules**) pick the account automatically, for example “everything from github.com/my-company uses the Work account”. They are checked from top to bottom.",
              "Each account can have several **commit identities** (name + email written into commits) — e.g. a work email and a personal one.",
              "A repository can use a different account per remote (shown under **Per remote** in the pill).",
            ],
          },
        ],
      },
      {
        id: "switch",
        title: "Switch account, or use another one just once",
        blocks: [
          {
            kind: "steps",
            items: [
              "Click the account pill.",
              "Choose **Use for this repo** (the repository keeps using it; tick **Remember** to lock it so rules cannot change it) or **Only for this action** (only the next commit / push uses it; afterwards the repository's usual account is back).",
              "Click the account (or one of its identities).",
            ],
          },
          {
            kind: "p",
            text: "Shortcut: Ctrl+Alt+1 … 9 binds the open repository to account 1 … 9 (the order of the list in the account manager). In PR / issue screens, the “as ● account ▾” button also has **Only for this action**.",
          },
        ],
      },
      {
        id: "ssh",
        title: "SSH keys",
        blocks: [
          {
            kind: "term",
            term: "SSH key",
            text: "a pair of files that proves who you are to GitHub / GitLab without typing a password; used by remotes that start with git@….",
          },
          {
            kind: "steps",
            items: [
              "Account manager → tab **Accounts** → **SSH keys…** on the account.",
              "**Generate key** (or **Import from file…**), then **Upload to provider** and **Test connection**.",
            ],
          },
          {
            kind: "p",
            text: "The app uses the right key for each account by itself — you do not need host aliases in ~/.ssh/config.",
          },
        ],
      },
      {
        id: "reauth",
        title: "Sign in again",
        blocks: [
          {
            kind: "p",
            text: "When a token expires or is revoked, the account shows “Needs sign-in” and errors offer a **Sign in again** button. You can also open the account manager and click **Re-authenticate** on the account. GitHub sign-ins are renewed automatically while the app runs.",
          },
        ],
      },
      {
        id: "export",
        title: "Export / import the account configuration",
        blocks: [
          {
            kind: "p",
            text: "Moving to another computer? Account manager → tab **Accounts** → **Export configuration…** saves your accounts, identities and rules to a file protected by a password you choose. On the other computer use **Import configuration…** with the same password. See also “Backup & moving to a new computer”.",
          },
        ],
      },
    ],
  },
  {
    id: "commit",
    title: "Commit your changes",
    summary: "Stage, unstage, discard, commit, amend, AI suggestions.",
    sections: [
      {
        id: "stage",
        title: "Choose what goes into the commit (stage)",
        blocks: [
          {
            kind: "term",
            term: "Commit",
            text: "a saved snapshot of your files with a short message saying what changed.",
          },
          {
            kind: "term",
            term: "Stage",
            text: "put a change in the basket of things the next commit will save; unstaged changes stay on your disk but are not saved.",
          },
          {
            kind: "list",
            items: [
              "Right panel → **Unstaged**: click **Stage** on a file (or **Stage all**).",
              "Staged by mistake? Under **Staged** click **Unstage** (or **Unstage all**).",
              "Click a file name to open its diff (what changed, line by line). In the diff you can stage only a hunk or only some lines: click the lines, then **Stage N lines**.",
              "**Undo** on an unstaged file throws away your edits to it (asks first; this cannot be undone).",
            ],
          },
        ],
      },
      {
        id: "write",
        title: "Write the message and commit",
        blocks: [
          {
            kind: "steps",
            items: [
              "Type a short **Summary** (what you did, e.g. “Fix login button on mobile”). Add details in **Description** if useful.",
              "Click **Commit N files** (or press Ctrl+Enter).",
            ],
          },
          {
            kind: "p",
            text: "Before committing, the app may show **Before you commit** if the name / email does not match the repository's account, and the **Large File Guard** if a file is very big (see “Large files & LFS”).",
          },
          {
            kind: "tip",
            text: "Type # in the message to pick an issue from the server: Enter inserts “#12”, Shift+Enter inserts “Closes #12” (the issue closes when the commit reaches the main branch).",
          },
        ],
      },
      {
        id: "amend",
        title: "Fix the last commit (amend)",
        blocks: [
          {
            kind: "term",
            term: "Amend",
            text: "replace the last commit with a corrected one (new message and/or extra files) instead of adding a new commit.",
          },
          {
            kind: "p",
            text: "Tick **Amend previous commit** under the message box, change the message or stage more files, then click **Amend commit**. Avoid amending a commit you already pushed: the server copy would differ from yours.",
          },
        ],
      },
      {
        id: "ai",
        title: "Commit messages written by AI ✨",
        blocks: [
          {
            kind: "steps",
            items: [
              "Turn it on once: **Settings → AI commit messages → AI settings…**, switch on **Turn on AI commit messages** and choose where the AI runs (your computer with Ollama — free and private — or a service such as OpenAI with your own API key).",
              "Stage your files, then click **✨** next to the summary box. The suggestion fills the message — read it and change it before committing.",
            ],
          },
          {
            kind: "warn",
            text: "The staged changes are sent to the AI service you chose. Do not use a cloud service for code you are not allowed to share; a local model keeps everything on your computer.",
          },
        ],
      },
    ],
  },
  {
    id: "branches",
    title: "Branches",
    summary: "Create, switch (double-click, automatic stash), merge, rebase, drag & drop, delete.",
    sections: [
      {
        id: "what",
        title: "What is a branch?",
        blocks: [
          {
            kind: "term",
            term: "Branch",
            text: "a separate line of work: you can try something on a branch without touching the main version, then combine it later.",
          },
          {
            kind: "term",
            term: "Checkout / switch",
            text: "move to another branch — the files in your folder change to that branch's version.",
          },
        ],
      },
      {
        id: "create",
        title: "Create a branch",
        blocks: [
          {
            kind: "steps",
            items: [
              "Click **Branch** on the toolbar (or right-click a commit → **Create branch here…**).",
              "Type a name without spaces, e.g. feature/login, keep **Check out the new branch** ticked and click **Create**.",
            ],
          },
        ],
      },
      {
        id: "switch",
        title: "Switch branch: double-click",
        blocks: [
          {
            kind: "steps",
            items: [
              "In the left panel under **Local**, **double-click** the branch name (or select it with the keyboard and press Enter). A single click only shows its latest commit in the graph.",
              "A branch only under **Remote** (e.g. origin/feature)? Double-click it too: the app makes the local copy that follows it, and switches to it.",
            ],
          },
          {
            kind: "p",
            text: "If you have uncommitted changes, the app asks what to do with them:",
          },
          {
            kind: "list",
            items: [
              "**Stash changes, then switch** (recommended): your changes are put aside safely (in a stash named “nanomgg-autostash: <branch>”), the switch happens, and when you come back to that branch they are **put back automatically**.",
              "**Bring the changes along**: the changes follow you to the other branch. If Git refuses because they would overwrite files there, the app tells you and offers the first option.",
              "Tick **Remember my choice** to stop asking. Change it later in **Settings → Branch switching** (Always ask / Always stash / Always bring along, and turn automatic restore on or off).",
            ],
          },
          {
            kind: "p",
            text: "The app also stops and explains when switching is not safe: a merge / rebase is half done (finish or abort it first), a pull / push is running (wait), the branch is open in another worktree (button **Open that worktree**), or you are on a commit that no branch keeps (it offers **Create a branch here first**).",
          },
          {
            kind: "tip",
            text: "After a switch the message has a **Back to <branch>** button. If the project has submodules that now point to other commits, the message offers **Update submodules**.",
          },
        ],
      },
      {
        id: "merge",
        title: "Combine branches: merge and rebase",
        blocks: [
          {
            kind: "term",
            term: "Merge",
            text: "bring the commits of another branch into the current one, keeping both histories.",
          },
          {
            kind: "term",
            term: "Rebase",
            text: "move your commits so they start from the newest commit of another branch, giving a straight history (your commits get new ids).",
          },
          {
            kind: "list",
            items: [
              "Right-click a branch → **Merge into <current>…**, choose how to merge, then **Merge**.",
              "Right-click → **Rebase <current> onto <branch>**. The app makes a backup first, so you can always undo.",
              "**Drag & drop**: drag a branch in the left panel (or its label in the graph) onto another branch, then choose Fast-forward, Merge or Rebase.",
              "**Interactive rebase** (right-click a commit → **Interactive rebase from here…**): reorder commits by dragging (or Alt+↑ / Alt+↓), and choose pick / reword / squash / fixup / drop / edit for each one.",
            ],
          },
          {
            kind: "p",
            text: "If both sides changed the same lines, Git stops with a conflict — see “Conflicts”.",
          },
        ],
      },
      {
        id: "delete",
        title: "Rename or delete a branch",
        blocks: [
          {
            kind: "p",
            text: "Right-click the branch → **Rename…** or **Delete…**. You cannot delete the branch you are on — switch to another one first. Deleting a branch whose commits are not merged anywhere asks again (**Delete anyway**). The branch menu can also delete the copy on the server, or both.",
          },
        ],
      },
    ],
  },
  {
    id: "sync",
    title: "Sync with the server",
    summary: "Fetch, pull, push, the ↑ / ↓ badges, automatic fetch, Push Guard.",
    sections: [
      {
        id: "words",
        title: "Fetch, pull and push",
        blocks: [
          {
            kind: "term",
            term: "Remote",
            text: "the copy of the repository on a server (GitHub / GitLab), usually called origin.",
          },
          {
            kind: "list",
            items: [
              "**Fetch**: check the server for news, without changing your files.",
              "**Pull**: fetch and bring the new commits into your current branch. The ▾ arrow offers merge (default), rebase or fast-forward only.",
              "**Push**: send your new commits to the server. The ▾ arrow has a safe **Force push** — use it only if you know why.",
            ],
          },
        ],
      },
      {
        id: "badges",
        title: "The ↑ / ↓ numbers",
        blocks: [
          {
            kind: "p",
            text: "↑3 on Push = 3 of your commits are not on the server yet. ↓2 on Pull = the server has 2 commits you do not have. Yellow means both — pull first. If you press Push while behind, the app offers **Pull, then push**.",
          },
          {
            kind: "p",
            text: "The app checks the server by itself (when a repository opens, every few minutes and when you come back to the window), silently and only with accounts that are signed in.",
          },
        ],
      },
      {
        id: "guard",
        title: "Push Guard",
        blocks: [
          {
            kind: "p",
            text: "Before pushing, **Before you push** may appear: the account cannot write to this repository, commits have another author than your identity (it can fix the author of unpushed commits), or files are too big for the server (it can move them to LFS — **Move to LFS** — with a backup you can undo).",
          },
          {
            kind: "p",
            text: "Running transfers are listed in the status bar (transfer queue): you can cancel, retry and copy error details there.",
          },
        ],
      },
    ],
  },
  {
    id: "conflicts",
    title: "Conflicts",
    summary: "When two changes touch the same lines: how to choose what to keep.",
    sections: [
      {
        id: "what",
        title: "What is a conflict?",
        blocks: [
          {
            kind: "term",
            term: "Conflict",
            text: "Git cannot decide alone because the same lines were changed differently on both sides; you choose the result.",
          },
          {
            kind: "p",
            text: "The yellow bar shows the paused operation and how many files are still in conflict. Conflicted files are listed under **Conflicts** in the right panel.",
          },
        ],
      },
      {
        id: "resolve",
        title: "Resolve step by step",
        blocks: [
          {
            kind: "steps",
            items: [
              "Click a file under **Conflicts** → **Resolve conflict**.",
              "At the top, “mine” and “theirs” side by side (read-only); at the bottom, the **Result** that will be saved. For each block click **Take** mine, **Take** theirs or **Take both**, or type in the result directly. ‹ › jump between blocks.",
              "Click **Save & mark resolved**. The next conflicted file opens by itself.",
              "When none are left, click **Continue** in the yellow bar (or **Abort** to go back to how things were before).",
            ],
          },
          {
            kind: "warn",
            text: "During a rebase Git swaps “ours” and “theirs”. The editor's labels say which is which — click **What are ours / theirs?** if unsure.",
          },
          {
            kind: "p",
            text: "Images and other binary files cannot be merged line by line: the editor shows both versions so you can keep one.",
          },
        ],
      },
    ],
  },
  {
    id: "history",
    title: "History",
    summary: "The graph, searching commits, file history, blame, viewing a file at a commit.",
    sections: [
      {
        id: "graph",
        title: "Read the graph",
        blocks: [
          {
            kind: "p",
            text: "Each row is a commit. Colored lines are branches; a node with two lines coming in is a merge. Labels show branches (laptop = local, cloud = server, ✓ = the branch you are on) and tags. Click a commit to see its details on the right; right-click for actions (checkout, create branch, cherry-pick, revert, reset, tag…).",
          },
          {
            kind: "term",
            term: "HEAD",
            text: "where you are now — usually the tip of the branch you are on.",
          },
          {
            kind: "term",
            term: "SHA",
            text: "the unique id of a commit, like 3f2a9c1.",
          },
        ],
      },
      {
        id: "search",
        title: "Find a commit",
        blocks: [
          {
            kind: "p",
            text: "Press Ctrl+F (or click the search box above the graph) and type words from the message, an author or a SHA. Enter / Shift+Enter jump to the next / previous match. Command palette → **Go to commit by SHA…** jumps directly.",
          },
        ],
      },
      {
        id: "file",
        title: "History of one file, blame, a file at an old commit",
        blocks: [
          {
            kind: "list",
            items: [
              "Right-click a file (in **Changes** or in **Commit details**) → **File history**: every commit that changed it.",
              "→ **Blame (who changed each line)**: each line with the commit and author that last changed it; click a line to see that commit in the graph.",
              "In file history, **View file at this commit** shows the whole file as it was then.",
            ],
          },
        ],
      },
    ],
  },
  {
    id: "stash",
    title: "Stash",
    summary: "Put unfinished changes aside and bring them back later.",
    sections: [
      {
        id: "use",
        title: "Use the stash",
        blocks: [
          {
            kind: "term",
            term: "Stash",
            text: "a drawer where Git keeps your unfinished changes so the folder is clean; you take them out later.",
          },
          {
            kind: "list",
            items: [
              "**Stash** on the toolbar: put your changes aside (optionally with a message and new files included).",
              "**Pop** on the toolbar: bring back the latest stash and remove it from the list.",
              "Left panel → **Stashes**: double-click a stash to see its files; right-click it to **Apply** (keep it in the list), **Pop** or **Drop** it.",
            ],
          },
          {
            kind: "tip",
            text: "Stashes named “nanomgg-autostash: <branch>” were made by the safe branch switch. They come back by themselves when you return to that branch.",
          },
        ],
      },
    ],
  },
  {
    id: "undo",
    title: "Undo / Redo",
    summary: "Take back the last operations: commit, merge, rebase, reset, branch changes…",
    sections: [
      {
        id: "use",
        title: "How it works",
        blocks: [
          {
            kind: "p",
            text: "**Undo** / **Redo** on the toolbar (Ctrl+Z / Ctrl+Shift+Z outside text boxes) take back or redo the last operation: commit, amend, reset, switching branch, merge, rebase, cherry-pick, revert, pull, creating / deleting / renaming a branch. Hover the button to see what it will undo.",
          },
          {
            kind: "list",
            items: [
              "Undoing a commit puts its changes back as staged — nothing is lost.",
              "Undo never overwrites uncommitted changes; if the repository changed since, it says so instead.",
              "If the commits were already pushed, the app asks first: undo only changes your computer.",
            ],
          },
        ],
      },
    ],
  },
  {
    id: "lfs",
    title: "Large files & LFS",
    summary: "Keep big files (images, videos, game assets) out of the history; Large File Guard.",
    sections: [
      {
        id: "what",
        title: "What is LFS?",
        blocks: [
          {
            kind: "term",
            term: "Git LFS",
            text: "an add-on that stores big files on a separate server and keeps only a small pointer in the history, so the repository stays light.",
          },
          {
            kind: "p",
            text: "Left panel → **LFS** shows whether the repository uses LFS and which file patterns it tracks. **Manage LFS…** opens the Git LFS window: **Files** (download files that are only pointers), **Locks** (lock a file so others do not edit it at the same time), **Patterns**, **Maintenance** (free disk space, check integrity).",
          },
        ],
      },
      {
        id: "guard",
        title: "Large File Guard",
        blocks: [
          {
            kind: "p",
            text: "When you commit a big file, the app stops and offers **Track with LFS (recommended)**, **Add to .gitignore** (do not save it in Git at all) or **Commit anyway** (not possible above the server's hard limit). The limits can be changed per server in the account manager → tab **Servers**.",
          },
          {
            kind: "tip",
            text: "Already committed a big file but not pushed? Push Guard offers **Move to LFS** for the unpushed commits.",
          },
        ],
      },
    ],
  },
  {
    id: "pulls",
    title: "Pull / merge requests & issues",
    summary: "Review and merge PRs / MRs, create issues, branch from an issue.",
    sections: [
      {
        id: "pr",
        title: "Pull requests (GitHub) / merge requests (GitLab)",
        blocks: [
          {
            kind: "term",
            term: "PR / MR",
            text: "a request to merge your branch into another one on the server, where teammates can comment and approve first.",
          },
          {
            kind: "list",
            items: [
              "Left panel → **PRs / MRs**: the list for this repository; click one to open it (Overview, Commits, Files changed).",
              "In a PR: comment, comment on a line (hover a line in Files changed), **Review** (approve / request changes), **Checkout** (get its branch locally), **Merge**.",
              "Create one: right-click your branch → **Start a pull request from <branch>…** (the app offers to push first), or after pushing a new branch.",
            ],
          },
        ],
      },
      {
        id: "issues",
        title: "Issues",
        blocks: [
          {
            kind: "p",
            text: "Left panel → **Issues**: filter, open, comment, close / reopen, create a **New issue**. In an issue, **Create a branch for this issue** makes a branch named like feature/123-title.",
          },
          {
            kind: "tip",
            text: "Every action shows “as ● account ▾”: click it to do this one action with another account.",
          },
        ],
      },
    ],
  },
  {
    id: "mywork",
    title: "My work & My notes",
    summary:
      "PRs waiting for you, your PRs, your issues — from every account — plus your own notes.",
    sections: [
      {
        id: "mywork",
        title: "My work",
        blocks: [
          {
            kind: "p",
            text: "Click **My work** on the toolbar. It gathers, from every signed-in account: PRs / MRs waiting for your review, your own PRs / MRs, and issues assigned to you. Each row has the color of its account; click it to open it. It refreshes every 5 minutes.",
          },
        ],
      },
      {
        id: "notes",
        title: "My notes",
        blocks: [
          {
            kind: "p",
            text: "In the same window, the **My notes** tab is a small notebook for yourself (to-dos, reminders, commands you keep forgetting), with tags to sort them. Notes stay on this computer; you can delete them in **Settings → Storage → Manage storage… → Your data in the app**.",
          },
        ],
      },
    ],
  },
  {
    id: "terminal",
    title: "Integrated terminal",
    summary: "Type commands when you need to, with the repository's account already set up.",
    sections: [
      {
        id: "use",
        title: "Use the terminal",
        blocks: [
          {
            kind: "p",
            text: "Click **Terminal** on the toolbar or press Ctrl+` (the key under Esc). It opens in the repository folder, and git commands typed there run as the repository's account. Copy / paste with Ctrl+Shift+C / Ctrl+Shift+V. You can open several terminals per repository.",
          },
        ],
      },
    ],
  },
  {
    id: "submodules",
    title: "Submodules & worktrees",
    summary: "Repositories inside a repository; several folders of the same repository.",
    sections: [
      {
        id: "submodules",
        title: "Submodules",
        blocks: [
          {
            kind: "term",
            term: "Submodule",
            text: "another repository embedded in a folder of yours, pinned to one exact commit.",
          },
          {
            kind: "p",
            text: "Left panel → **Submodules** shows each one with its state (not downloaded, up to date, other commit, has changes). **Update all** downloads them and checks out the commit your repository records. Right-click one to open it as a tab, sync its URL, deinit or remove it. Each submodule uses the account that matches its own URL.",
          },
        ],
      },
      {
        id: "worktrees",
        title: "Worktrees",
        blocks: [
          {
            kind: "term",
            term: "Worktree",
            text: "a second folder of the same repository with another branch checked out, so you can work on two branches at once.",
          },
          {
            kind: "p",
            text: "Left panel → **Worktrees** → **Add worktree…** (or right-click a branch → **Create worktree from <branch>…**). Open it as a tab, lock it, or remove it. A branch can be checked out in only one worktree at a time.",
          },
        ],
      },
    ],
  },
  {
    id: "signing",
    title: "Signing commits",
    summary: "Prove that commits really come from you (GPG or SSH).",
    sections: [
      {
        id: "setup",
        title: "Set up signing",
        blocks: [
          {
            kind: "steps",
            items: [
              "Account manager → tab **Accounts** → the account's **Commit identities** → **Edit**.",
              "Under **Sign commits** choose GPG or SSH and pick the key, then click **Test signing**.",
            ],
          },
          {
            kind: "p",
            text: "Commit details then show a **Signature** line (Verified, Untrusted key, Not signed…). For SSH signing, also add the public key as a “signing key” in your GitHub / GitLab settings on the website so the website shows “Verified”.",
          },
        ],
      },
    ],
  },
  {
    id: "appearance",
    title: "Appearance & custom themes",
    summary: "Light, dark, follow the system, high contrast or your own colors; language.",
    sections: [
      {
        id: "theme",
        title: "Theme and language",
        blocks: [
          {
            kind: "steps",
            items: [
              "**Settings** → **Theme**: Light / Dark / System / **Custom**.",
              "**Manage themes…** opens the theme editor: start from **High contrast — Dark** or **High contrast — Light**, or make your own colors.",
              "**Language**: English or Tiếng Việt (changes at once).",
            ],
          },
          {
            kind: "p",
            text: "Also in Settings: show CI results (off by default) and author avatars on the graph.",
          },
        ],
      },
    ],
  },
  {
    id: "backup",
    title: "Backup & moving to a new computer",
    summary: "Export the account configuration, back up and restore the whole app.",
    sections: [
      {
        id: "full",
        title: "Back up everything",
        blocks: [
          {
            kind: "steps",
            items: [
              "**Settings → Backup & restore → Create backup…**, choose a password and where to save the file.",
              "On the new computer: install the app, then **Settings → Backup & restore → Restore from backup…** and enter the same password.",
            ],
          },
          {
            kind: "warn",
            text: "Keep the password somewhere safe: without it the backup cannot be opened. Your code is not in the backup — it lives in your repositories (push it, or copy the folders).",
          },
        ],
      },
      {
        id: "accounts",
        title: "Only the accounts",
        blocks: [
          {
            kind: "p",
            text: "Account manager → tab **Accounts** → **Export configuration…** / **Import configuration…** moves just the accounts, identities and rules.",
          },
        ],
      },
    ],
  },
  {
    id: "storage",
    title: "Storage & clean-up",
    summary: "Where the app keeps its logs, cache and data, and how to free space.",
    sections: [
      {
        id: "folders",
        title: "Folders of the app",
        blocks: [
          {
            kind: "p",
            text: "**Settings → Storage → Manage storage…** shows the size and place of the **Logs**, **Cache** and **App data**. You can move each one to another drive (**Change…**), go **Back to default**, **Clear all cache** (nothing is lost) or **Delete old logs**. Your repositories are never stored or deleted here.",
          },
          {
            kind: "p",
            text: "The section **Your data in the app** lists what you created inside the app (custom themes, notes) so you can delete items one by one or all at once.",
          },
          {
            kind: "tip",
            text: "The app warns you when the drive holding its data has less than 1 GB free.",
          },
        ],
      },
    ],
  },
  {
    id: "updates",
    title: "Updating the app",
    summary: "Check for a new version and install it.",
    sections: [
      {
        id: "check",
        title: "Check for updates",
        blocks: [
          {
            kind: "p",
            text: "The app checks at most once a day after starting. You can also go to **Settings → Updates** and click the check button, or use the command palette → **Check for updates**. When a version is available: **Update & restart**, **Later** or **Skip this version**. Choose **Stable** (recommended) or **Beta** under Update channel.",
          },
          {
            kind: "p",
            text: "If this build cannot update itself, it says so and links to the Releases page: download the new installer and install it over this one — your data and accounts are kept.",
          },
        ],
      },
    ],
  },
  {
    id: "shortcuts",
    title: "Keyboard shortcuts",
    summary: "The keys that save the most time. On macOS use ⌘ instead of Ctrl.",
    sections: [
      {
        id: "global",
        title: "Everywhere",
        blocks: [
          {
            kind: "keys",
            rows: [
              ["Ctrl+K", "Command palette: find and run any command"],
              ["Ctrl+Z / Ctrl+Shift+Z", "Undo / redo the last operation (not inside text boxes)"],
              ["Ctrl+B", "Show / hide the left panel"],
              ["Ctrl+Alt+B", "Show / hide the right panel"],
              ["Ctrl+`", "Show / hide the terminal"],
              ["Ctrl+Alt+1 … 9", "Use account 1 … 9 for the open repository"],
            ],
          },
        ],
      },
      {
        id: "work",
        title: "While working",
        blocks: [
          {
            kind: "keys",
            rows: [
              ["Ctrl+F", "Search commits in the graph (Enter / Shift+Enter: next / previous)"],
              ["↑ ↓ PageUp PageDown Home End", "Move through commits in the graph"],
              ["Esc", "Close commit details / the diff"],
              ["Enter (on a branch in the left panel)", "Switch to that branch"],
              ["Ctrl+Enter", "Commit (in the message box); send a comment"],
              ["#", "In the commit message: suggest issues (Enter: #12, Shift+Enter: Closes #12)"],
              ["Alt+↑ / Alt+↓", "Move a commit in interactive rebase"],
              ["Ctrl+Shift+C / Ctrl+Shift+V", "Copy / paste in the terminal"],
            ],
          },
        ],
      },
    ],
  },
  {
    id: "faq",
    title: "Questions & troubleshooting",
    summary: "Common problems and what to click.",
    sections: [
      {
        id: "push-rejected",
        title: "Push was rejected: “the remote has newer commits”",
        blocks: [
          {
            kind: "p",
            text: "Someone pushed before you. Click **Pull** (resolve conflicts if any), then **Push** again — or use **Pull, then push**. Avoid force push unless you are sure.",
          },
        ],
      },
      {
        id: "auth",
        title: "“Authentication failed” or “no access”",
        blocks: [
          {
            kind: "p",
            text: "Check the account pill: is this the account that has access to the repository? Switch it, or click **Sign in again** in the message. Pushing files under .github/workflows needs the “workflow” permission on GitHub — the message has a button to grant it.",
          },
        ],
      },
      {
        id: "wrong-author",
        title: "My commits show the wrong name / email",
        blocks: [
          {
            kind: "p",
            text: "Choose the right identity in the account pill, or add a rule. For commits not pushed yet, Push Guard can fix their author before pushing.",
          },
        ],
      },
      {
        id: "cannot-switch",
        title: "I cannot switch branch",
        blocks: [
          {
            kind: "p",
            text: "Read the message: finish or abort the merge / rebase in the yellow bar, wait for the pull / push to end, or choose **Stash changes, then switch** when you have uncommitted changes. My changes disappeared after switching? They are in the stash “nanomgg-autostash: <branch>”: go back to that branch (they return by themselves) or Pop them from **Stashes**.",
          },
        ],
      },
      {
        id: "detached",
        title: "“Detached HEAD”",
        blocks: [
          {
            kind: "term",
            term: "Detached HEAD",
            text: "you are on one commit, not on a branch; new commits made there are easy to lose.",
          },
          {
            kind: "p",
            text: "Create a branch there (right-click the commit → **Create branch here…**) or double-click an existing branch to go back.",
          },
        ],
      },
      {
        id: "lfs-pointers",
        title: "Files contain only a few lines of text (LFS pointers)",
        blocks: [
          {
            kind: "p",
            text: "Git LFS is missing or the files were not downloaded. Install git-lfs (**Settings → Getting started** checks it), then **Manage LFS… → Files** and download them.",
          },
        ],
      },
      {
        id: "report",
        title: "Something went wrong",
        blocks: [
          {
            kind: "p",
            text: "Open **Technical details** in the message to see Git's own words. The logs are in **Settings → Storage → Manage storage… → Logs → Open folder** — attach today's file when reporting a bug.",
          },
        ],
      },
    ],
  },
];
