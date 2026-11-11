"""The feedback each run sent: every user message that follows a tool result
in the request bodies the fake gateway kept (capture.sh's `gwlog`), with the
tool it follows. The bodies stay out of the receipt (their system prompt names
the run's directory); this prints only the body's file name and the message.

usage: python3 -I sent.py <side>/<bin>-<scenario>-<size>-bodies...   (relative, from the
capture root; printed as given)
"""

import json
import os
import sys

for directory in sys.argv[1:]:
    assert not os.path.isabs(directory), "give the bodies relative to the capture root"
    print("## %s" % os.path.normpath(directory))
    for name in sorted(os.listdir(directory)):
        body = json.load(open(os.path.join(directory, name), encoding="utf-8"))
        prompt = body.get("prompt") or []
        for i, message in enumerate(prompt):
            if i and message.get("role") == "user" and prompt[i - 1].get("role") == "tool":
                tools = [part.get("toolName") for part in prompt[i - 1]["content"]]
                texts = [part.get("text") for part in message["content"] if part.get("type") == "text"]
                print("%s: after %s, the user message %s" % (name, ",".join(tools), json.dumps(texts)))
