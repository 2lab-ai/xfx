import json,sys
seen=[]
for l in open(sys.argv[1]):
    m=json.loads(l)
    if m.get("reason")!="compiler-message": continue
    msg=m["message"]
    code=(msg.get("code") or {}).get("code")
    if code!="clippy::disallowed_methods": continue
    sp=[s for s in msg["spans"] if s["is_primary"]][0]
    exp=sp.get("expansion"); chain=[]
    while exp:
        d=exp.get("def_site_span") or {}
        chain.append(f'{exp["macro_decl_name"]} called at {exp["span"]["file_name"]}:{exp["span"]["line_start"]}, defined at {d.get("file_name")}:{d.get("line_start")}'); exp=exp["span"].get("expansion")
    key=(m["target"]["name"], sp["file_name"], sp["line_start"])
    print(f'{"/".join(m["target"]["kind"])} {m["target"]["name"]}: {sp["file_name"]}:{sp["line_start"]} {msg["level"]}', "; ".join(chain) if chain else "(no macro)")
