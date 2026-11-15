#!/usr/bin/env python3
# Writes a git fast-import stream: about N commits with a realistic graph
# (feature branches merged back, a few long-lived branches in parallel).
import sys
N = int(sys.argv[1])
out = sys.stdout.buffer
mark = 0
t = 1_500_000_000
def commit(ref, parent, merges, msg, path):
    global mark, t
    mark += 1; t += 60
    m = msg.encode(); content = f"{mark}\n".encode()
    out.write(b"commit %s\nmark :%d\ncommitter Rocky <rocky@example.com> %d +0000\ndata %d\n%s\n" % (ref.encode(), mark, t, len(m), m))
    if parent: out.write(b"from :%d\n" % parent)
    for p in merges: out.write(b"merge :%d\n" % p)
    out.write(b"M 100644 inline %s\ndata %d\n%s\n" % (path.encode(), len(content), content))
    return mark
main = commit("refs/heads/main", 0, [], "Initial commit", "a.txt")
longs = []
for i in range(3):
    longs.append(commit(f"refs/heads/long{i}", main, [], f"Start long-lived branch {i}", f"long{i}.txt"))
n = 0
while mark < N:
    n += 1
    main = commit("refs/heads/main", main, [], f"Change {n} on main", f"m{n%3}.txt")
    # Long-lived branches move along, merging into main now and then.
    i = n % 3
    longs[i] = commit(f"refs/heads/long{i}", longs[i], [], f"Work {n} on long {i}", f"long{i}.txt")
    if n % 97 == 0:
        main = commit("refs/heads/main", main, [longs[i]], f"Merge long {i} into main", f"m{n%3}.txt")
    if n % 20 == 0:
        tip = main
        for k in range(3):
            tip = commit("refs/heads/tmp", tip, [], f"Feature {n} step {k}", f"feature{n%5}.txt")
        main = commit("refs/heads/main", main, [tip], f"Merge feature {n}", f"m{n%3}.txt")
out.write(b"reset refs/heads/tmp\nfrom 0000000000000000000000000000000000000000\n")
