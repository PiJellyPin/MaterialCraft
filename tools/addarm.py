#!/usr/bin/env python3
"""helper used while porting nodes: python3 tools/addarm.py KINDS ARMFILE  (inserts the arm text before the final `_ => None`)"""
import sys
kinds = sys.argv[1].split(",")
arm = open(sys.argv[2]).read()
p = "src/graph_gpu.rs"
s = open(p).read()
for k in kinds:
    if '"%s"' % k not in s.split("fn supported")[1].split("/// Runs node")[0]:
        s = s.replace('            | "shape_drop_shadow"\n', '            | "shape_drop_shadow"\n            | "%s"\n' % k, 1)
end = "        _ => None,\n    }\n}"
i = s.rindex(end)
s = s[:i] + arm.rstrip("\n") + "\n" + s[i:]
open(p, "w").write(s)
