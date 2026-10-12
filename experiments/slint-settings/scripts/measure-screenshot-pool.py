"""Pools a results file by overlay and cores: pool.py FILE

The time to the frozen screen over all screenshots; and, for each kind of
selection, the run whose 95th is the middle one of the runs made.
"""
import json
import statistics
import sys

frozen, later, runs = {}, {}, {}
for line in open(sys.argv[1]):
    row = json.loads(line)
    key = row["label"].split(", round")[0].split(", again")[0]
    frozen.setdefault(key, []).extend(row["frozen_ms"])
    later.setdefault(key, []).extend(ready - up for up, ready in zip(row["frozen_ms"], row["ready_ms"]))
    if row["selection"] != "open":
        runs.setdefault((key, row["selection"]), []).append(row)
for key, values in frozen.items():
    late = sorted(later[key])
    print(f"{key}: {len(values)} screenshots, frozen median {statistics.median(values)}, {min(values)} to {max(values)}; ready later by up to {late[-1]}, {sum(1 for v in late if v >= 19)} of {len(late)} by 19 or more")
for (key, selection), rows in runs.items():
    def middle(of):
        """The run whose 95th of that is the middle one, of the runs that have it."""
        having = sorted((row for row in rows if row.get(of)), key=lambda row: row[of][1])
        return having[len(having) // 2][of] if having else None

    print(f"{key}, {selection} ({len(rows)} runs): jump {middle('jump_ms')}  sweep {middle('sweep_ms')}  frame {middle('frame_ms')}")
