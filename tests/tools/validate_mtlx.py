#!/usr/bin/env python3
"""Check .mtlx files with the official MaterialX library (pip install MaterialX==1.39.4 - the version Unreal Engine 5.8
reads). Usage: validate_mtlx.py file.mtlx [...]. Exit code 1 when a file is invalid or refers to a missing texture."""
import os, sys
import MaterialX as mx

lib = mx.createDocument()
mx.loadLibraries(['libraries'], mx.FileSearchPath(os.path.dirname(mx.__file__)), lib)
bad = 0
for f in sys.argv[1:]:
    doc = mx.createDocument()
    mx.readFromXmlFile(doc, f)
    doc.importLibrary(lib)
    ok, msg = doc.validate()
    base = os.path.dirname(os.path.abspath(f))
    missing = []
    for e in doc.traverseTree():
        if e.isA(mx.Input) and e.getType() == 'filename' and e.getValueString() and not os.path.isfile(os.path.join(base, e.getValueString())):
            missing.append(e.getValueString())
    mats = [m for m in doc.getMaterialNodes()]
    status = ok and not missing and len(mats) >= 1
    print(('OK   ' if status else 'FAIL ') + f, '' if ok else msg, ('missing: %s' % missing) if missing else '', '(%d material)' % len(mats))
    bad += 0 if status else 1
sys.exit(1 if bad else 0)
