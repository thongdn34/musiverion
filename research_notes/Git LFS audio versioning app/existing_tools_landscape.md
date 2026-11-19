# Competitive Landscape: Versioning & Collaboration Tools for Music/Audio Projects (and Binary-Asset VCS Analogues)

Research date: 2026-11-19. About 18 tool calls. Many entries rely on vendor pages or roundup blogs, and these are flagged. Product names in the brief that returned nothing are listed under Gaps.

## 1. Music-specific tools: what exists, features, pricing, status

### Takeaway
Music collaboration tools fall into four groups. (a) Project sync/versioning: Splice Studio (dead 2023), Blend (dead 2024), Ohm Studio (dead), Avid Cloud Collaboration (status unclear). (b) Real-time streaming/remote sessions: Audiomovers, VST Connect, Sessionwire, LANDR Sessions. (c) Browser DAWs with built-in collaboration: BandLab, Soundtrap. (d) Bounce review/approval tools: Pibox, Notetracks, Samply, Feedtracks. The sync/versioning group, which is closest to a git+LFS app, has the worst survival record. A wave of small "git for music" projects appeared in 2025–2026, but none has shipped at scale.

### Cited Findings

**Splice Studio (DISCONTINUED 2023)**
- Splice launched in 2014 with Studio, its cloud collaboration and backup feature, as the centrepiece. It worked with Ableton Live, Logic Pro X, FL Studio, GarageBand and Studio One. Users could add stems, bounces and collaborators, and sessions and revisions synced automatically. — [MusicRadar](https://musicradar.com/news/splice-studio-closing)
- Shutdown phases: by 10 Mar 2023, users could no longer add stems, bounces or collaborators or create new web projects. On 7 Apr 2023, the desktop app stopped accepting new projects. On 31 May 2023, the Projects tab was removed and syncing stopped. — [MusicRadar](https://musicradar.com/news/splice-studio-closing). Splice's own blog gives June 5, 2023 as the final date, so the sources disagree by a few days. — [Splice blog, CEO letter](https://splice.com/blog/announcing-splice-studio-v2/)
- Stated reasons, from CEO Kakul Srivastava: Studio "hasn't been a focus for us since 2017"; "we haven't been able to provide the quality of experience of which we can be proud"; keeping it running "has actually slowed us down from delivering more value, faster"; and "our users have many great alternatives for file sharing available to them." Splice moved its focus to the sample marketplace and rent-to-own plugins. — [MusicRadar](https://musicradar.com/news/splice-studio-closing); [Splice blog](https://splice.com/blog/announcing-splice-studio-v2/)
- Since the closure, third parties such as Boombox market themselves as "Splice Studio alternatives". This is promotional material. — [boombox.io](https://boombox.io/?p=232)

**Blend.io (ACQUIRED 2015, DISCONTINUED June 2024)**
- Backed by Betaworks, Blend launched in 2013 as a way to collaborate "without ever abandoning Ableton". — [TechCrunch 2013](https://techcrunch.com/2013/04/24/betaworks-backed-blend-io-lets-musicians-collaborate-without-ever-abandoning-ableton)
- ROLI acquired it in Oct 2015 for undisclosed terms. By then it supported 13 DAWs. — [TechCrunch 2015](https://techcrunch.com/2015/10/23/roli-makers-of-the-new-seaboard-rise-acquire-betaworks-backed-blend-io); [Synthtopia](https://www.synthtopia.com/content/2015/10/27/roli-acquires-blend-social-collaboration-platform/)
- It shut down in 2024. Users had to download their collaborative projects through the Blend desktop app's sync function by Tuesday 4 June 2024. No reason was given. — [CDM](https://cdm.link/2024/05/endlesss-discontinued/)

**Endlesss (DISCONTINUED 31 May 2024)**
- Tim Exile's collaborative loop-jamming app. "Endlesss Ltd will cease operations at the end of May." Services ended 31 May 2024, and users were told to export stems/video beforehand. — [MusicTech](https://musictech.com/news/gear/tim-exile-endlesss-app-shut-down/); [CDM](https://cdm.link/2024/05/endlesss-discontinued/)
- CDM's analysis: online collaboration "really *can't* be provided without a service". The opportunity it sees is linking "a sustainable business in production tools to an appealing service option for connections". It also notes that Endlesss "really did connect with people", so user demand was not the stated problem. — [CDM](https://cdm.link/2024/05/endlesss-discontinued/)

**Ohm Studio (DEFUNCT)**
- Official site: "the Ohm Studio adventure is over"; the server would shut down Sept 30. An official post from 18 Sep 2020 gives application access ending 12-31-2020, so the dates conflict. The company said it never met a demand strong enough to be commercially sustainable. Users were told to download projects locally. Two community members got an offline (PC-only) build working. — [ohmstudio.com](https://www.ohmstudio.com/); [ohmstudio.com/content/end](http://www.ohmstudio.com/content/end)

**Avid Cloud Collaboration / Pro Tools (status UNCLEAR)**
- Avid's product page still describes Cloud Collaboration as letting "multiple collaborators contribute to the same Pro Tools project remotely". It lists support for Pro Tools Artist/Studio/Ultimate. The page is undated. — [Avid](https://cn.avid.com/pro-tools/cloud-collaboration)
- Pro Tools | First, the free tier tied to free cloud accounts, was discontinued in Dec 2021. Coverage says the collaboration infrastructure was expensive and time-consuming to maintain. Avid also cleared older projects from free cloud storage accounts. — [Production Expert](https://www.production-expert.com/home-page/pro-tools-first-discontinued-what-does-this-mean?format=amp); [Mackie](https://mackie.com/en/news/all/pro_tools_first_and_mackie_discontinued.html); [Pro Tools Expert](https://www.pro-tools-expert.com/home-page/2040/6/12/pro-tools-first-avid-warn-they-will-be-clearing-down-older-projects-in-free-cloud-storage-accounts)
- Production Expert ran a warning article, "Use Pro Tools Cloud Collaboration? You should read this". Its content was not retrieved. — [Production Expert](https://pro-tools-expert.squarespace.com/production-expert-1/use-pro-tools-cloud-collaboration-you-should-read-this)

**Pibox (ACTIVE): review/approval inside Pro Tools**
- Opens as a tab in the Pro Tools side panel with projects, versions, comments and approvals. You "bounce straight from your session and share one link", and every new bounce lands under the same link as a new version. Tagline: "every mix and every cut, signed off in one place". (Vendor page.) — [pibox.com](https://pibox.com/)
- Capterra reviews are mostly positive on ease of use and version/comment handling. One complaint: with 20 comments across projects you have to open each one to clear the markers. — [Capterra](https://www.capterra.com/p/151696/Pibox/reviews/)
- One review says the platform "packs extensive features that require time to learn". — [Audome blog](https://blog.audome.com/samply-app-alternatives-5/)

**Notetracks (ACTIVE)**
- Lets you share projects and invite collaborators to leave timestamped comments on audio. Comments can be exported as text. Marketed to teams as "Notetracks Pro". — [notetracks.com](https://www.notetracks.com/)

**Samply (ACTIVE, per a 2026 alternatives article)**
- No independent review found. A 2026 "Samply alternatives" article exists, which suggests the product is still around. — [Audome blog](https://blog.audome.com/samply-app-alternatives-5/)

**LANDR Sessions (ACTIVE)**
- A video-chat plus DAW-streaming plugin, not a versioning tool. A review cites latency problems and notes that collaborators can't manipulate the host's DAW. — [Producer Hive](https://producerhive.com/buyer-guides/daw/landr-sessions-review/); [Audio Sorcerer](https://audiosorcerer.com/post/landr-sessions/)
- LANDR acquired Reason Studios in January 2026. — [Wikipedia: Reason Studios](https://en.wikipedia.org/wiki/Reason_Studios)

**Audiomovers LISTENTO (ACTIVE)**
- Streams high-quality audio from the DAW to a browser for real-time listening. Listening is free and the sender gets a 7-day trial. It does not version or share projects. — [AlternativeTo / roundups](https://alternativeto.net/software/vst-connect/); [Zoundroom 2026](https://zoundroom.com/en/blog/best-apps-for-remote-songwriting-collaboration)

**Steinberg VST Connect (ACTIVE, Cubase-centric)**
- Built for remote recording and producing with Cubase. It was first bundled with Cubase 7 and later sold as a standalone Pro product. No 2026 updates found. — [Wikipedia: Cubase](https://en.wikipedia.org/wiki/Cubase); [AlternativeTo](https://alternativeto.net/software/vst-connect/)

**Sessionwire (status UNVERIFIED)**
- An audio/video interface for real-time work across different DAWs. A 2019-era listing gave a 30-day trial, then $15/mo or $150/yr. Current pricing and status are unconfirmed. — [Hypebot](https://www.hypebot.com/9-music-collaboration-apps-we-love-right-now/)

**Kompoz (ACTIVE per roundups)**
- An online collaboration community with over 100,000 registrations and subscription revenue (free Starter tier plus paid Plus/Premier/Pro). Projects are moving to private by default, with collaboration requests left public. — [Hypebot](https://www.hypebot.com/top-7-collaborative-music-making-apps/); [MI.edu](https://www.mi.edu/in-the-know/8-collaborative-music-apps-make-music-anyone-anywhere/)

**Trackd (status UNVERIFIED)**
- A mobile-first (iOS) app with an 8-track studio, lyric pad and in-app collaboration. Dave Stewart of Eurythmics promoted it in 2020. — [PR Newswire 2020](https://prnewswire.com/news-releases/dave-stewart-of-eurythmics-offer-musicians-in-isolation-chance-to-collaborate-using-trackd-music-app-301036850.html); [netted.net 2016](https://www.netted.net/2016/07/26/trackd-music-creation-app/)

**BandLab (ACTIVE)**
- A free web DAW with real-time chat and "version control" (project revisions). BandLab Membership costs $14.99/mo on the US App Store and covers premium features, not basic collaboration. — [Zoundroom 2026](https://zoundroom.com/en/blog/best-apps-for-remote-songwriting-collaboration); [RP Music Studios](https://www.rpmusicstudios.com/blog/how-to-remote-music-collaboration-tools-and-tips)

**Soundtrap (ACTIVE)**
- An Oct 2026 update added third-party VST3/AU effects to collaborative projects. Collaborators can hear the processed track without owning the plugin. Instruments and master-channel plugins are not supported yet (instruments are "planned"), and the feature is not in the free plan. — [Bedroom Producers Blog, 12 Oct 2026](https://bedroomproducersblog.com/2026/10/12/soundtrap-third-party-plugins/)

**Feedtracks (ACTIVE, review/versioning for audio)**
- Publishes content marketing on "version control for audio". It pitches stacked versions with notes per version and lets clients compare. It argues that "Final_Mix_v3_REAL_FINAL_MASTER.wav" naming gives "no metadata, no timeline, no comparison". — [Feedtracks blog](https://feedtracks.com/blog/version-control-nightmare-how-producers-can-stop-losing-final-mixes); [Feedtracks blog 2](https://feedtracks.com/blog/version-control-for-audio-projects-never-lose-a-mix-again)

**2024–2026 "git for music" entrants (all early stage / open source)**
- **StemHub**: "Git-style version control and collaboration platform for music producers". A DAW plugin handles commit/push/pull/branch, and a web platform adds cloud storage and cross-DAW exports. — [GitHub stemhub-org/Stemhub](https://github.com/stemhub-org/Stemhub)
- **Wit** ("Git for waves", sep-lab, Apache-2.0): local-first, with no account or server. It stores project files as delta chains (about 11 KB/save), source audio as content-addressed objects (FLAC and chunking recommended), and renders/freezes as a prunable cache. It produces semantic diffs such as "TEMPO 120.0 -> 124.0 BPM". An "Ears" tier null-diffs two bounces. Parser status by DAW:
  - Ableton (.als, gzipped XML) is best.
  - Logic/GarageBand: the container is decoded and Logic is the first app target.
  - FL Studio is good through v24, but v25 adds obfuscation.
  - Cubase has no parser for modern files.
  - Pro Tools is obfuscated and carries legal risk.
  
  It is pre-release with nothing downloadable: 3 stars, 55 commits. — [GitHub sep-lab/Wit](https://github.com/sep-lab/Wit)
- Wit's measurements against git: a localized edit adds about 1.1 MB, but a global gain change adds about 16.8 MB (a full second copy). "LFS does not fix that; it just moves the copies elsewhere." On .als project XML, git came within 3× of Wit's delta chain. Most of the 26 GB → 12 GB storage saving came from deduplication and FLAC, not from history (about 0.3 GB). — [GitHub sep-lab/Wit](https://github.com/sep-lab/Wit)
- **kit-vcs**: "Git for music producers" aimed at sample/loop kits, which producers otherwise reship as a new zip for every update. — [GitHub kit-vcs](https://github.com/nojanpourjafar/kit-vcs)
- **gitcrusher**: a minimal git wrapper for music projects. — [GitHub gitcrusher](https://github.com/monkybrain/gitcrusher)
- **Logic-tools**: uses a Dropbox folder as the git "remote" for distributed Logic Pro teams. — [GitHub autholykos/logics](https://github.com/autholykos/logics)
- **GitDaw**: a browser DAW with git-style branching, commits and merge previews. — [gitdaw.net](https://gitdaw.net/git-daw)
- A University of Washington iSchool capstone poster on "Git Music" exists, which shows academic interest. — [UW iSchool PDF](https://ischool.uw.edu/sites/default/files/capstone/posters/19%20Git%20Music.pdf)
- A June 2026 SoundisiaK blog guide frames the difference: git tracks text diffs, while music version control tracks full audio files with A/B playback comparison. — [SoundisiaK blog](https://blog.soundisiak.studio/2026/06/17/what-is-version-control-for-music.html)

**General sync (Dropbox/Drive) and plain git**
- Dropbox keeps version history automatically, and some producers sync the whole projects folder. Reaper project files are human-readable text, so plain git works well, and some producers keep private GitHub/GitLab repos with Git LFS for audio. (Source is a vendor blog.) — [Feedtracks](https://feedtracks.com/blog/version-control-for-audio-projects-never-lose-a-mix-again)

### Inferences
- Every tool that tried hosted project-level sync and versioning (Splice Studio, Blend, Ohm Studio, and Avid's free cloud tier) has been shut down or scaled back. The survivors are either narrow (bounce review: Pibox, Notetracks, Samply), real-time streaming (Audiomovers), or tied to their own DAW (BandLab, Soundtrap). This looks like a structural pattern: storing and syncing large multi-DAW projects is expensive and hard to make good, while bounce review is cheap and easy to monetise.
- A git + Git LFS app with storage on infrastructure the team owns or chooses (GitHub/GitLab/self-hosted LFS) avoids the main failure mode: users lost their projects when the vendor's servers turned off. Exportability and data ownership should be headline features, given that Splice, Blend, Endlesss and Ohm all ended with "download your projects now".
- Splice's note that "users have many great alternatives for file sharing" suggests plain file sync was seen as a commodity. A new app's value has to come from what Dropbox lacks: named versions, A/B bounce comparison, approvals, locking, and per-version notes.
- Wit's DAW-format findings set a hard ceiling. Semantic project diffs are feasible for Ableton (XML) and Reaper (text), partly for Logic, and hard or risky for FL 25, Cubase and Pro Tools. A practical app should treat project files as opaque binaries with locking, and offer audio-level comparison (A/B playback, null-diff of bounces) across all DAWs.
- Git LFS stores a full copy of every changed audio file. A team app needs history pruning, an "only latest locally" mode, and storage-quota visibility to keep costs under control.

### Gaps
- Nothing found for "Vocalfy", "Bounce Boss", "Muzie", "Tonal" (as a music VCS), "Sonic Versioning", or "Avid Sync". They may not exist under those names or may be too small to index.
- Current status and pricing for Avid Cloud Collaboration, Sessionwire, Trackd and Samply could not be confirmed for 2026.
- No current pricing found for Pibox, Notetracks, Samply or Audiomovers.
- Splice's explicit storage and backup terms were not captured.

## 2. Game/media binary-asset version control: products and UX patterns for non-technical users

### Takeaway
Anchorpoint is the closest analogue to the proposed app: a git + LFS desktop GUI for artists. It auto-configures LFS, offers its own server-side file locking (not Git LFS locking), shows thumbnails in history, supports sparse/selective checkout, and lets users prune local binaries. It costs about €20–25 per user per month. Diversion, Unity Version Control (Gluon) and Perforce compete with centralized models that have exclusive locking as a core feature for artists.

### Cited Findings

**Anchorpoint (ACTIVE)**
- Pricing: a free Personal tier for non-commercial, single-user use. The Team plan costs €20/user/mo billed annually or €25/user/mo billed monthly. It includes Git (and Lore) for major hosting providers, file locking, selective checkout and Git LFS configuration. — [Anchorpoint facts/policies](https://www.anchorpoint.app/policies/facts). Indies get 50% off. — [Fab listing](https://www.fab.com/listings/c37f3c5e-70c6-4ea0-bb18-b1b367ec443c)
- Locking: Anchorpoint does not use Git LFS locking. It runs its own lock system on its metadata server and warns that LFS locking "should not be used in conjunction with" it. — [Anchorpoint docs: file locking](https://docs.anchorpoint.app/git/file-locking/); [Anchorpoint blog: file locking in git](https://www.anchorpoint.app/blog/using-file-locking-in-git). A competitor page claims the opposite. — [usourcecontrol.com](https://usourcecontrol.com/compare/anchorpoint) (contradicted by Anchorpoint's own docs)
- Locks are centralized and "require no prior configuration". Users "see a lock icon directly in the UI", and the app stops a push if the user hasn't acquired the lock. — [Anchorpoint blog: version control for binary files](https://www.anchorpoint.app/blog/version-control-for-binary-files)
- UX patterns:
  - "Automating the technical setup of LFS, allowing artists to avoid the command line"; the app "automatically configures the .gitattributes file".
  - Fast previews for video, 3D and textures.
  - Tags and metadata for "visual discovery".
  - Pruning of old binaries locally while history stays on the server.
  - A commit is explained as "like making a checkpoint for your project". — [Anchorpoint blog](https://www.anchorpoint.app/blog/version-control-for-binary-files)
- More UX patterns:
  - A visual folder panel.
  - Thumbnails (FBX, PSD, uasset) in the commit history.
  - Sparse checkout to "sync only the files they need".
  - Unity/Unreal plugins to commit and see locks inside the engine.
  - It "understands industry-relevant image, video, audio, and 3D file formats", and its sample folder structure has Audio/Music/SFX folders.
  - It keeps git vocabulary ("commit, push and pull") but syncs locks, tags, reviews and notifications as metadata.
  - Runs on Windows and macOS. Works with GitHub, Azure DevOps and others, plus Anchorpoint's managed "Lore" hosting. — [Anchorpoint: git for artists](https://www.anchorpoint.app/features/git-for-artists)
- Anchorpoint states the core problem: LFS "solves the storage problem" but "doesn't address the 'human' problem for non-technical artists", and two versions of a binary asset can't be merged. — [Anchorpoint blog](https://www.anchorpoint.app/blog/version-control-for-binary-files)
- A newer entrant, "Lockstep" (GitHub sign-in, "coming soon"), targets the same git-locking niche. — [lockstep-marketing.pages.dev](https://lockstep-marketing.pages.dev/)

**Diversion (ACTIVE)**
- A cloud-native VCS marketed as artist-friendly. Pricing page:
  - Free Indie tier: first 5 users, up to 20 users total, 100 GB free storage.
  - Pro: $25/user/mo.
  - Studio: $35/user/mo.
  
  Hard locks come only on Studio and above. Another Diversion page quotes "Free tier + $19/seat", so the vendor's own pages are inconsistent. It claims up to 70% lower cost than Perforce (vendor claim). — [Diversion pricing](https://www.diversion.dev/pricing); [Diversion comparison](https://www.diversion.dev/knowledge-center-articles/perforce-vs-plastic-scm-vs-diversion-which-is-right-for-your-studio); [Diversion home](https://www.diversion.dev/)

**Unity Version Control (formerly Plastic SCM) (ACTIVE)**
- Gluon is its simplified client "for non-developers", with file-based checkout and locking. Locks are enforced once lock rules are set. The cloud plan reportedly dropped per-seat charges in March 2026: a free tier with 25 GB storage and 100 GB egress, then pay-as-you-go. An older page says 3 free users, 5 GB and $7/user, which is probably outdated. — [Assembla 2026](https://get.assembla.com/blog/version-control-for-game-development/); [Nerdisa](https://nerdisa.com/unity); [Wikipedia](https://en.wikipedia.org/wiki/Unity_Version_Control)

**Perforce Helix Core / P4 (ACTIVE)**
- Free for up to 5 users. Exclusive file locking is what "artists rely on". The P4V client is "widely criticized" for UX. Self-hosting costs, including infrastructure and IT staff, are the highest of the group. — [Assembla 2026](https://get.assembla.com/blog/perforce-vs-plasticscm-indie-game-dev-teams/)
- An industry take: "Your VCS isn't your real problem". Asset review and discovery workflows matter as much as the VCS. — [ArtStash](https://artstash.io/resources/perforce-vs-git-vs-unity-vcs-isnt-your-real-problem)

### Inferences
- The proven UX recipe for non-technical users:
  1. Zero-config LFS (auto `.gitattributes` by file extension).
  2. Server-side locks, shown as an icon with the holder's name, and enforced before push.
  3. Thumbnails and previews in history. For music this means waveform plus inline playback of the bounce.
  4. Selective or sparse checkout and local pruning.
  5. Integration inside the creative tool. For music that means a DAW plugin, as Pibox does in Pro Tools and StemHub plans.
  6. Plain-language explanations of git concepts.
- Locking matters more for DAW projects than for game assets, because DAW project files (.als, .logicx, .flp, .ptx) can't be merged. A "check out to edit / auto-lock on open" model like Perforce and Gluon fits a band better than git's optimistic branching.
- Anchorpoint built its own lock server instead of using Git LFS locks. That suggests LFS locking is either badly supported by hosts and clients or too limited. A new app should decide early between LFS locks (which need host support, e.g. GitHub/GitLab) and its own lightweight lock service.
- Pricing anchors for a team tool: €20–35/user/mo (Anchorpoint, Diversion) with free tiers for small teams. A musician team is likely more price-sensitive than a game studio.

### Gaps
- I did not research Frame.io, Lucid or other media review tools (timecoded comments, approvals). Treat them as known analogues for the bounce-review UX, without citations here.
- No primary evidence was found for a "sync/publish" label replacing commit/push in Anchorpoint. Its page still uses commit/push/pull terminology.
- No user-sentiment data (e.g. Reddit or G2) on Anchorpoint or Diversion was gathered.

## 3. Open-source / mainstream git GUIs with LFS: fit for non-programmers

### Takeaway
GitHub Desktop and GitKraken handle LFS files once they are set up, but setup needs the command line and neither offers proper lock management. Locking is a Git LFS CLI feature, which is why artist-focused tools (Anchorpoint) exist.

### Cited Findings
- GitHub Desktop: "To use Git LFS with GitHub Desktop, you must configure Git LFS using the command line." After that, LFS files behave like any other file. — [GitHub Docs](https://docs.github.com/en/enterprise-cloud@latest/desktop/configuring-and-customizing-github-desktop/about-git-large-file-storage-and-github-desktop)
- GitKraken: a user feature request says there is no way to interact with LFS locks. It is listed as "under consideration". — [GitKraken feedback](https://feedback.gitkraken.com/suggestions/220717/support-git-lfs-large-file-storage-locking-feature)
- Git LFS locking (since 2.0) makes "lockable" files read-only locally until someone runs `git lfs lock path`. — [git-lfs wiki: File Locking](https://github.com/git-lfs/git-lfs/wiki/File-Locking); [GitHub blog: LFS 2.0](https://github.blog/2017-03-02-git-lfs-2-0-0-released/)
- GitHub's web UI does not respect LFS locks, according to a community complaint. — [DEV Community](https://dev.to/devactivity/unlocking-productivity-why-githubs-web-ui-must-respect-git-lfs-locks-2afb)
- The community built workarounds for artists, such as Unity Git locks plugins and Gitlit. — [unity-git-locks](https://github.com/KAW0/unity-git-locks); [Gitlit](https://awesome.ecosyste.ms/projects/github.com%2F5im-0n%2Fgitlit)
- Anchorpoint's comparison page pitches itself against GitHub Desktop on locking and LFS setup. (Vendor claim.) — [Anchorpoint vs GitHub Desktop](https://www.anchorpoint.app/compare/alternative-to-github-desktop)

### Inferences
- Even "friendly" git GUIs leave artists with LFS setup, lock handling, `.gitattributes` and merge conflicts on binaries. A musician app has to hide all of these.

### Gaps
- Fork and Sourcetree LFS/lock support was not verified.
- No first-hand studies or surveys of non-programmers using these GUIs were found.

## 4. What musicians and producers say they need

### Takeaway
The pain points that come up again and again:
- Version naming chaos ("final_final_v3_REAL").
- Knowing which version the client approved.
- Comparing bounces.
- Stems handoff to mastering or clients.
- Keeping projects portable between machines.

Forum evidence is anecdotal. Reddit threads did not show up in search.

### Cited Findings
- Version naming chaos: "v3 had a problem, so you fixed it but kept the version number... Then the client approved it, so naturally: 'Final_Mix_v3_REAL_FINAL_MASTER.wav'". The article cites "no metadata, no timeline, no comparison" and recommends per-version notes and shared version stacks. (Vendor blog.) — [Feedtracks](https://feedtracks.com/blog/version-control-nightmare-how-producers-can-stop-losing-final-mixes)
- Approval-to-mastering handoff: a Gearspace moderator says that once the client approves, mastering gets the approved mix plus a version without the limiter. Another poster suggests stems that null-sum 1:1 to the mix. — [Gearspace: reinforcing mix elements with stems](https://gearspace.com/threads/reinforcing-mix-elements-with-stems.1452024/page-2#post-17518184)
- Stems ownership is contested. Some say clients have no right to stems unless agreed in advance; others say stems are owed if the client pays for the time. Tiered offers (dry, bus and FX stems) are proposed. — [Gearspace: Do you give out stems](https://gearspace.com/threads/do-you-give-out-stems-to-clients.915664/); [Gearspace: client wants mix files](https://gearspace.com/threads/client-wants-mix-files.1106641/)
- Sample and plugin consistency: Soundtrap's 2026 feature exists so that "your collaborator can hear the processed track without having the same plugin". This shows plugin mismatch is a known blocker. — [Bedroom Producers Blog](https://bedroomproducersblog.com/2026/10/12/soundtrap-third-party-plugins/)
- Sample-kit versioning: producers reship kits as new zips for every update (the motivation for kit-vcs). — [kit-vcs](https://github.com/nojanpourjafar/kit-vcs)
- Real-time collaboration limits: LANDR Sessions users report latency, and remote collaborators can't control the host DAW. — [Producer Hive](https://producerhive.com/buyer-guides/daw/landr-sessions-review/)
- Review-tool UX friction: Pibox users complain about clearing comment markers across many projects. — [Capterra](https://www.capterra.com/p/151696/Pibox/reviews/)

### Inferences
- The app's core objects should be: project snapshot (opaque DAW file and audio), bounce attached to the snapshot, per-version note, approval status (e.g. "client approved" or "sent to mastering"), and stems package. "Which version did the client approve" is best answered with an explicit approval tag or label tied to a commit, plus a shareable listen link.
- Bounce A/B and null-diff (as in Wit's "Ears") give a meaningful "diff" for audio across every DAW.
- Plugin and sample manifests per project (which plugins and versions, which samples, and missing-file checks) would address a known gap. None of the surveyed tools except Soundtrap, and only inside its own ecosystem, deals with plugin mismatch.

### Gaps
- No Reddit (r/WeAreTheMusicMakers, r/audioengineering, r/edmproduction) or KVR threads were found. Search returned none, so first-hand sentiment on "git for music" from those communities is missing.
- No quantitative survey data was found on how producers version projects.

## 5. Business and viability lessons: why music collaboration startups fail or pivot

### Takeaway
Shutdowns cluster around the same reasons:
- Server-dependent collaboration costs a lot to run.
- Demand was too weak to sustain it commercially.
- Parent companies shifted focus to more profitable lines (samples, hardware, DAW subscriptions).
- Users treated file sync as a commodity.

### Cited Findings
- Splice moved away from Studio toward samples and rent-to-own. Studio had not been a focus since 2017 and keeping it alive "slowed us down". — [MusicRadar](https://musicradar.com/news/splice-studio-closing)
- Ohm Studio "never met a demand strong enough to be commercially sustainable". — [ohmstudio.com](https://www.ohmstudio.com/)
- Avid ended free Pro Tools First and cleared old free-cloud projects. The collaboration infrastructure was reportedly costly to maintain. — [Production Expert](https://www.production-expert.com/home-page/pro-tools-first-discontinued-what-does-this-mean?format=amp); [Pro Tools Expert](https://www.pro-tools-expert.com/home-page/2040/6/12/pro-tools-first-avid-warn-they-will-be-clearing-down-older-projects-in-free-cloud-storage-accounts)
- Blend was acquired by a hardware company (ROLI) and quietly sunset in June 2024. Endlesss closed in May 2024 despite an engaged community. CDM says the open question is linking "a sustainable business in production tools" to the collaboration service. — [CDM](https://cdm.link/2024/05/endlesss-discontinued/); [TechCrunch](https://techcrunch.com/2015/10/23/roli-makers-of-the-new-seaboard-rise-acquire-betaworks-backed-blend-io)
- Survivors charge subscriptions for narrow, high-value workflows: Kompoz has paid tiers on a 100k+ user base, Sessionwire charged $15/mo, and Audiomovers charges senders while listeners are free. — [Hypebot](https://www.hypebot.com/top-7-collaborative-music-making-apps/); [Hypebot](https://www.hypebot.com/9-music-collaboration-apps-we-love-right-now/); [Zoundroom](https://zoundroom.com/en/blog/best-apps-for-remote-songwriting-collaboration)

### Inferences
- Lessons for a git + LFS team app:
  1. Don't run the storage yourself, or keep it optional. Use the team's own git host or LFS server so the app can't strand users' work.
  2. Keep the scope narrow: versioning, locking, bounce review and approvals, not a social network or real-time jam.
  3. Make archives open and portable (plain git repos are portable by design).
  4. Charge for convenience such as lock service, review links and hosted LFS, not for access to the user's own data.
- Social and community collaboration (Blend, Kompoz, Endlesss, Ohm) has a poor track record. A tool for a closed, known team (a band or studio) fits better with the B2B game-dev model (Anchorpoint, Diversion), which charges per seat.

### Gaps
- No funding or revenue figures were found for Blend, Endlesss or Ohm Studio, and no post-mortems from their founders.
- No reason was given for Blend's shutdown.
