# TOME user guide

TOME is a desktop client for the [Lore](https://github.com/EpicGames/lore) version control system. This guide walks a first-time user from opening a working copy to changing it and sharing the work with the team. TOME's screens are in Korean; the labels below are given in Korean with their meaning.

[한국어](ko.md)

## 1. Install and first run

- **Installer**: `TOME_<version>_x64-setup.exe` installs for the current user. No administrator rights are needed.
- **Portable**: unzip and run `tome.exe`. Windows 11 already has the WebView2 runtime it needs.

On the first run a setup window opens.

1. **Identity (신원)**: the name recorded as the author of your commits. On a server without authentication this name is all that is kept, so use one per machine (`kim-pc`, `kim-laptop`) to tell where a commit came from.
2. **Server (서버)**: the team's Lore server (`lore://host:41337`). **연결 확인** (check connection) lists the server's repositories.
3. **Working copy**: choose **이미 있는 작업본 열기** (open an existing working copy) and pick its folder, then **시작** (start). Or choose **서버에서 받기** (get from the server), pick a repository and a folder, then **받고 열기** (get and open). Getting a copy shows progress and can be cancelled at any time (the partial folder is removed).

The gear button at the top right opens the settings again. They are stored in `%APPDATA%\dev.ldnjin.tome`.

## 2. The window

- **Left**: branches (`main` first, then the current branch; names with a prefix such as `auto/` are grouped and can be folded away), the working copy, your identity and whether live notifications are on.
- **Middle**: tabs. Programmer mode has **Smartlog · 변경 (changes) · 잠금 (locks)**; artist mode has **에셋 (assets) · 변경 · 잠금**. Switch with **프로그래머/아티스트** in the toolbar.
- **Right**: details of the selected revision or asset.
- **Status bar**: the `lore` command line for what was just done, if you want to do the same in a terminal.

## 3. Smartlog: my stack

**내 스택** (my stack) shows your commits that are not pushed yet (drafts) against the server's latest revision.

- **Server row** at the top: appears when the server has revisions you do not have. **Sync** gets them.
- **Uncommitted changes**: files you changed show as a dashed purple card. **새 커밋** (new commit) goes to the changes tab.
- **draft**: your commits; the top one is the one you are working on.
- **내 스택의 베이스** (base of my stack): the server revision your drafts sit on.

**전체 그래프** (full graph) shows every branch in lanes. Select a revision to see its changed files on the right; select a file to see its diff.

## 4. Change, commit, push

1. In the **변경** tab, check the changed files. **diff** shows a file's change, **기록** (history) its earlier revisions.
2. Stage the files to commit (**모두 스테이징** stages all).
3. Write a message and press **커밋** (commit) or **커밋 후 push** (commit, then push).

Push and Sync can take a while: a progress window opens and **취소** (cancel) stops part way. Running it again continues where it stopped.

## 5. When the server has new revisions (restack or merge)

If you have drafts and someone else pushed first, the branch has diverged and your push is refused. The server row offers two ways:

- **내 스택을 위로 옮기기 (restack)**: apply your drafts again, in order, on top of the server's new revisions, so the history stays a straight line. You can also drag a draft onto the server row.
- **병합해서 받기 (sync)**: get the server's revisions and make one merge revision.

Restack first shows a **preview**; nothing has changed yet.

- For each commit it lists files that may conflict. Text files are merged automatically when possible. For binary files (uasset and the like) you can choose up front to keep your change (**내 변경 유지**) or the base's version (**베이스 버전 사용**) if they conflict.
- Commits that change files someone else has locked are marked.
- A change the new base already has is left out rather than kept as an empty commit.
- A merge revision is flagged: moved, it becomes an ordinary commit and the merge link is lost.

A conflict you did not settle up front stops in the **Restack 중** panel at the top. For each file choose keep my change, use the base's version, or **직접 고침** (I edited it by hand), then **계속** (continue). **중단하고 되돌리기** (abort) puts everything back as it was. A stopped restack survives closing TOME. If something fails part way, TOME puts the branch and files back by itself.

## 6. Reorder and fold commits

- **Reorder**: drag a draft above or below another one in your stack. The preview's ▲▼ buttons adjust it further.
- **Fold**: hover a draft and press **아래와 합치기** (fold with the one below). Edit the joined message and press **합치기** (fold); the two become one commit and the commits above are applied again on top. Fold repeatedly to combine more than two.

Reordering and folding need a working copy without uncommitted changes. All of this stays in your working copy until you push.

## 7. Branches and merges

- **+ 새 브랜치** (new branch) creates a branch at the current revision (and can switch to it).
- Hover a branch for **전환** (switch) and **병합** (merge). Switching is refused while you have uncommitted changes.
- A merge with conflicts shows a **병합 중** (merging) bar at the top; in the changes tab, settle each file as **내 것** (mine), **상대 것** (theirs) or **수정 완료** (edited). Commit once all are settled, or **병합 중단** (abort merge) to go back.

## 8. Locks

Binary files cannot be merged, so lock a file before you change it.

- The **잠금** tab shows the whole team's locks. Select files and press **잠금** (lock) or **해제** (unlock); in the asset view the buttons are **잠그기** and **잠금 풀기**.
- Locked files are also marked in the changes tab and on asset cards.
- When someone locks, unlocks or pushes, a notice appears at the bottom right (while **실시간 알림 켜짐**, live notifications on, shows on the left).

On a server without authentication Lore cannot record who holds a lock, so the owner shows as **알 수 없음** (unknown).

## 9. Artist mode

Choose **아티스트** in the toolbar and the **에셋** tab comes first.

- Pick a folder in the tree on the left; its `.uasset` and `.umap` files show as thumbnail cards. Thumbnails are the images saved in each file, read without the editor; blueprints, sounds and maps that save no image show their kind instead.
- Cards are marked modified, added, conflicted or locked. **변경·잠금만** shows only those; the slider sets the card size.
- Select a card for a large thumbnail and its details on the right, where you can lock, unlock and see the file's history.
- Folders with thousands of assets stay light: only the cards on screen are drawn.

## 10. View (what to get)

**View** in the toolbar sets which part of the repository the working copy holds. A plain line excludes, a line starting with `!` includes again. Applying it removes unchanged files that fall outside and gets back files that come in again. Files you changed are never removed.

## 11. Custom tools

Like P4V's custom tools, TOME can run your own programs. Create them in **도구 ▾ → 도구 관리** (manage tools).

- Choose where a tool shows (working copy menu, file right-click, revision right-click) and how it runs (capture the output, a new console window, or detached).
- Arguments take variables such as `%f` (selected files), `%r` (revision), `%b` (branch) and `%a` (ask when run). The exact command line is shown before it runs.
- Project tools live in `.tome/tools.json` in the working copy, to share with the team. A project tool you have not seen, or whose file changed, runs only after you review it and press **신뢰** (trust).

## 12. When something goes wrong

TOME explains common Lore errors in Korean with what to do, keeping the original in parentheses. For example:

- **Lore 서버에 연결할 수 없습니다** (cannot reach the server): check the server address and the network. Work that needs no server can go on with **오프라인** (offline) on.
- **서버에 새 리비전이 있어 push할 수 없습니다** (the server has new revisions): restack or merge as in section 5.
- **커밋하지 않은 변경이 있어 …** (uncommitted changes): commit or revert them, then try again.
- **다른 사람이 잠근 파일입니다** (locked by someone else): see who holds it in the locks tab.

## 13. Good to know

- **오프라인** (offline) shows only what this computer has, without asking the server. Use it when the server is slow or down.
- The command lines in the status bar can be pasted into a terminal as they are.
- TOME is not made or endorsed by Epic Games.
