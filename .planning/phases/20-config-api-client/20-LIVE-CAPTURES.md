# Phase 20 — Live Captures (8.3.6)

## C-S3 — carried S3 redundancy pair (8.3.6; pre- and post-failover)

Source: `/Users/pmannion/data/worktrees/whk-environment-orchestration/gateway-config-spec/spikes/gateway-modes/results/S3.md` (spike date 2026-10-09). Carried from the gateway-modes spike S3 (clean 8.3.6 pair), not captured on this phase's single-gateway rig.

# S3.md:6-7 state: paired, no mode (pre-failover)

~~~
a={"role":"Master","projectState":"Good","activityLevel":"Active","localId":"192.168.97.3","peerId":"spike-a-Backup","peerConnected":true,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":29279,"lastSyncTimestamp":1791586091827}  mode_a=''
b={"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.97.2","peerId":"spike-a-Master","peerConnected":true,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1791586091875}  mode_b=''
~~~

# S3.md:40-41 state: after failover (post-failover)

~~~
a={"role":"Master","projectState":"Good","activityLevel":"Cold","localId":"192.168.97.3","peerId":"spike-a-Backup","peerConnected":true,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1791586358959}  mode_a='production'
b={"role":"Backup","projectState":"Good","activityLevel":"Active","localId":"192.168.97.2","peerId":"spike-a-Master","peerConnected":true,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":30539,"lastSyncTimestamp":1791586359045}  mode_b='production'
~~~

Role stays fixed across the failover (a is always Master, b is always Backup) while `activityLevel` moves (a Active to Cold, b Cold to Active). `lastSyncTimestamp` (1791586091827 and similar) is epoch-ms.
