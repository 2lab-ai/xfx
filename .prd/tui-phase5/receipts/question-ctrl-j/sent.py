"""The answer documents each run sent: every `ask_user_question` tool result
in the request bodies the fake gateway kept (capture.sh's `gwlog`), each
printed once, with the body that first carried it. The bodies stay out of the
receipt (their system prompt names the run's directory); this prints only the
body's file name and the tool result's text.

usage: python3 -I sent.py <side>/<bin>-<scenario>-<size>-bodies...   (relative, from the
capture root; printed as given)

APPROVAL-CTRL-J's sent.py, reading the question's tool results where that one
read the feedback after an approval.
"""

import json
import os
import sys

for directory in sys.argv[1:]:
    assert not os.path.isabs(directory), "give the bodies relative to the capture root"
    print("## %s" % os.path.normpath(directory))
    seen = set()
    for name in sorted(os.listdir(directory)):
        body = json.load(open(os.path.join(directory, name), encoding="utf-8"))
        for message in body.get("prompt") or []:
            if message.get("role") != "tool":
                continue
            for part in message.get("content") or []:
                if part.get("toolName") != "ask_user_question":
                    continue
                key = (part.get("toolCallId"), json.dumps(part.get("output"), sort_keys=True))
                if key in seen:
                    continue
                seen.add(key)
                output = part.get("output") or {}
                print("%s: %s %s" % (name, output.get("type"), output.get("value")))
