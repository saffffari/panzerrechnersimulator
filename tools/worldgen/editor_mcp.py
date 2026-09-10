#!/usr/bin/env python3
"""Repository-local MCP stdio authoring bridge. No shell execution or live-game control.

One JSON-RPC message per UTF-8 line; stdout is reserved for protocol messages. Every mutation
creates a new validated world revision. The native editor can open the returned manifest path.
"""
import contextlib
import json
import sys

import world_authoring as world

PROTOCOL="2025-06-18"
MAX_LINE_BYTES=4*1024*1024


def schema(properties, required=()):
    return {"type":"object","properties":properties,"required":list(required),"additionalProperties":False}


STRING={"type":"string"}
TOOLS=[
    ("pizza_asset_search","Find owned Pizza models by path/name; results expose only cataloged bounds, never guessed scale.",schema({"query":STRING,"limit":{"type":"integer","minimum":1,"maximum":100}})),
    ("world_inspect","Read a world, stable placement IDs, terrain bounds, markers, sources and revision hash.",schema({"manifest":STRING,"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":100}},["manifest"])),
    ("world_fork","Create a new immutable world revision, preserving the selected base and its overlay.",schema({"manifest":STRING,"name":STRING},["manifest","name"])),
    ("scene_validate","Validate an authored scene block's assets, terrain, historical provenance and real sources.",schema({"source":STRING},["source"])),
    ("scene_cook","Cook a single local scene into a separate playable study world; keep historical relocation explicit.",schema({"source":STRING,"name":STRING,"act_id":{"type":"integer","enum":[1,2,3]}},["source","name"])),
    ("world_transaction","Apply move/remove/place/sculpt/stamp_scene/sound/light operations by stable identity and expected revision hash. Publish a new world only if the complete transaction validates.",schema({"manifest":STRING,"expected_hash":STRING,"name":STRING,"operations":{"type":"array","maxItems":4096,"items":{"type":"object"}}},["manifest","expected_hash","name","operations"])),
    ("playtest_generate","Create a 768 m diagnostic composite with Pizza interiors, ruins, bare groves and range zones. Does not replace the menu default or any existing world.",schema({"name":STRING,"seed":{"type":"integer"}},["name","seed"])),
    ("act_regenerate","Create a new authored level study on the source act's macro-heightfield and markers. Rebuild suitable Pizza modules and bare groves; source overlays remain preserved, not index-rebased. CPU recipe, no render.",schema({"manifest":STRING,"name":STRING,"seed":{"type":"integer"}},["manifest","name","seed"])),
]


def invoke(name,a):
    entry=next((tool for tool in TOOLS if tool[0]==name),None)
    if entry is None:raise ValueError("unknown tool")
    contract=entry[2]
    if not isinstance(a,dict) or set(a)-set(contract["properties"]) or set(contract["required"])-set(a):raise ValueError("invalid tool arguments")
    for key,value in a.items():
        rule=contract["properties"][key];kind=rule["type"]
        valid=isinstance(value,str) if kind=="string" else isinstance(value,int) and not isinstance(value,bool) if kind=="integer" else isinstance(value,list) if kind=="array" else isinstance(value,dict)
        if not valid:raise ValueError(f"invalid {key} type")
        if "minimum" in rule and value<rule["minimum"]:raise ValueError(f"{key} below bound")
        if "maximum" in rule and value>rule["maximum"]:raise ValueError(f"{key} exceeds bound")
        if "enum" in rule and value not in rule["enum"]:raise ValueError(f"unsupported {key}")
    if name=="pizza_asset_search":return {"assets":world.asset_search(a.get("query",""),a.get("limit",40))}
    if name=="world_inspect":return world.inspect_world(a["manifest"],a.get("offset",0),a.get("limit",40))
    if name=="world_fork":return world.fork_world(a["manifest"],a["name"])
    if name=="scene_validate":
        scene=world.validate_scene(world.load_json(a["source"]))
        return {"source":a["source"],"sha256":world.digest(scene),"valid":True,"rendered":False,"items":len(scene["items"]),"provenance":scene["metadata"]["provenance"]}
    if name=="scene_cook":return world.cook_scene(a["source"],a["name"],a.get("act_id",2))
    if name=="world_transaction":return world.apply_transaction(a["manifest"],a["expected_hash"],a["name"],a["operations"])
    if name=="playtest_generate":return world.make_playtest(a["name"],a["seed"])
    if name=="act_regenerate":return world.regenerate_act(a["manifest"],a["name"],a["seed"])


def handle(message):
    ident=message.get("id")
    method=message.get("method","")
    if "id" not in message:return None
    if message.get("jsonrpc")!="2.0":return {"jsonrpc":"2.0","id":ident,"error":{"code":-32600,"message":"Invalid Request"}}
    if method=="initialize":result={"protocolVersion":PROTOCOL,"capabilities":{"tools":{}},"serverInfo":{"name":"panzerrechnersystem-world-authoring","version":"0.1.0"},"instructions":"Mutations publish new validated revisions. Open returned manifests in WORLDBUILDER or select them for Playtest. This server does not move a live editor camera, launch GPU captures or claim historical reconstruction accuracy."}
    elif method=="ping":result={}
    elif method=="tools/list":result={"tools":[{"name":n,"description":d,"inputSchema":s,"annotations":{"readOnlyHint":n in ("pizza_asset_search","world_inspect","scene_validate"),"destructiveHint":False,"openWorldHint":False}} for n,d,s in TOOLS]}
    elif method=="tools/call":
        try:
            params=message.get("params",{})
            with contextlib.redirect_stdout(sys.stderr):value=invoke(params.get("name"),params.get("arguments",{}))
            result={"content":[{"type":"text","text":json.dumps(value,allow_nan=False)}],"structuredContent":value,"isError":False}
        except Exception as error:result={"content":[{"type":"text","text":str(error)}],"isError":True}
    else:return {"jsonrpc":"2.0","id":ident,"error":{"code":-32601,"message":"Method not found"}}
    return {"jsonrpc":"2.0","id":ident,"result":result}


def main():
    while True:
        line=sys.stdin.buffer.readline(MAX_LINE_BYTES+1)
        if not line:break
        if len(line)>MAX_LINE_BYTES:
            print("MCP request exceeds 4 MiB bound; closing input",file=sys.stderr);break
        try:
            message=json.loads(line,parse_constant=lambda value:(_ for _ in ()).throw(ValueError(value)))
            response=handle(message) if isinstance(message,dict) else {"jsonrpc":"2.0","id":None,"error":{"code":-32600,"message":"Invalid Request"}}
        except (ValueError,TypeError) as error:response={"jsonrpc":"2.0","id":None,"error":{"code":-32700,"message":"Parse error: "+str(error)}}
        if response is not None:print(json.dumps(response,allow_nan=False),flush=True)


if __name__=="__main__":main()
