# 02 — Unreal Engine MCP Bridge Survey & Recommendation (UE 5.8 / Project "Kessel")

Status: research only. No editor launched, no build run. API claims about UE 5.8 are
verified against on-disk plugin descriptors at `C:/Program Files/Epic Games/UE_5.8/Engine/Plugins`.
Community-project claims are from web sources (post-cutoff) and are marked as such.

## TL;DR / Recommendation

- **PRIMARY: Use UE 5.8's first-party, built-in MCP server — the `ModelContextProtocol`
  plugin ("Unreal MCP"), plus `ToolsetRegistry` + `AllToolsets`.** It is shipped by Epic,
  on-disk now, speaks Streamable-HTTP MCP on `127.0.0.1:8000/mcp`, and auto-generates the
  `.mcp.json` for Claude Code via a console command. No third-party C++ plugin to maintain,
  no 5.8-compat risk. VERIFIED on-disk:
  `Engine/Plugins/Experimental/ModelContextProtocol/ModelContextProtocol.uplugin`
  (FriendlyName "Unreal MCP", `IsExperimentalVersion: true`, `EnabledByDefault: false`).
- **FALLBACK: `runreal/unreal-mcp`** (Node MCP server over Unreal's built-in Python Remote
  Execution). No custom C++ plugin — it drives the editor through `PythonScriptPlugin`'s
  remote-exec TCP, giving full `unreal` Python API access. Best when you need a tool the
  first-party toolsets don't expose yet, or want to script arbitrary editor automation.
- **AVOID as primary:** the C++-plugin community bridges (chongdashu, flopperam/Aura, kvick).
  All are pinned to UE ≤5.7 and ship their own C++ `UnrealMCP` plugin that must compile
  against the engine — i.e. exactly the 5.8 ABI/compile risk we want to avoid, and we cannot
  compile C++ at all until VS 2022 is installed.

Both recommended options require ZERO custom C++ compilation, which matters because this
machine has no MSVC yet (per environment ground truth).

---

## Part A — First-party UE 5.8 building blocks (VERIFIED on-disk)

### A1. `ModelContextProtocol` — "Unreal MCP" (the official server)
- Path: `Engine/Plugins/Experimental/ModelContextProtocol/ModelContextProtocol.uplugin`.
- Descriptor facts (VERIFIED, file lines 5–18): FriendlyName `Unreal MCP`; Description
  "Anthropic MCP (Model Context Protocol) server implementation for Unreal Engine.";
  `IsExperimentalVersion: true`; `EnabledByDefault: false`; `NoRedist: true`.
- Modules (lines 19–50): `ModelContextProtocol` (Runtime), `ModelContextProtocolEngine`
  (Runtime), `ModelContextProtocolEditor` (Editor) + three test modules.
- Hard dependency (lines 51–60): `ToolsetRegistry` (auto-enabled), `EngineAssetDefinitions`.
- Transport (web, Epic docs): **HTTP + Server-Sent Events only; `stdio` and WebSocket are
  NOT supported.** Binds `http://127.0.0.1:8000/mcp`, loopback-only, no auth by default.
- Client wiring (web, Epic docs): run console command
  `ModelContextProtocol.GenerateClientConfig ClaudeCode` in the editor → writes a `.mcp.json`
  into the project root that Claude Code auto-discovers.

### A2. `ToolsetRegistry` — tool catalog backbone (VERIFIED on-disk)
- Path: `Engine/Plugins/Experimental/ToolsetRegistry/ToolsetRegistry.uplugin`.
- The MCP server itself exposes NO tools; tools come from "toolset" plugins registered here.
  Same registry feeds other in-engine AI surfaces, so a toolset works across all of them.

### A3. `AllToolsets` + individual toolsets (VERIFIED on-disk)
- Path: `Engine/Plugins/Experimental/Toolsets/AllToolsets/AllToolsets.uplugin`
  (`EditorOnly: true`, `IsExperimentalVersion: true`, `EnabledByDefault: false`).
- `AllToolsets` is an aggregator that enables 21 toolset plugins. VERIFIED siblings on-disk
  under `Engine/Plugins/Experimental/Toolsets/` include those directly relevant to Milestone 1:
  - `EditorToolset` — general editor/actor ops (spawn, transform, inspect).
  - `PhysicsToolsets` — physics setup (relevant to tank drive/penetration rigs).
  - `MaterialInstanceTools` capability + `PCGToolset`, `NiagaraToolsets` (VFX/fog material work).
  - `SlateInspectorToolset` — read back UMG/Slate (the multi-sight HUD).
  - `UMGToolSet` — build/edit widgets (gunsight overlays, CRT instruments).
  - `AutomationTestToolset`, `GameplayTagsToolset`, `ConfigSettingsToolset`,
    `StateTreeToolset`, `AIModuleToolset`, `GASToolsets`, `GameFeaturesToolset`,
    `DataRegistryToolset`, `DataflowAgent`, `PluginToolset`, `SemanticSearchToolset`,
    `ConversationToolset`, `AnimationAssistantToolset`, `WorldConditionsToolset`,
    `MCPClientToolset`.
- You can enable `AllToolsets` for the broadest surface, or cherry-pick toolsets to keep the
  tool list (and token overhead) small. For Kessel M1, a minimal set is `EditorToolset` +
  `PhysicsToolsets` + `UMGToolSet`/`SlateInspectorToolset` + material/Niagara toolsets.
- Epic docs capability summary (web): "spawning actors, configuring lighting, creating
  material instances, inspecting Slate widgets, and running automation tests."

### A4. `RemoteControl` API — first-party HTTP/WebSocket control (VERIFIED on-disk)
- Path: `Engine/Plugins/VirtualProduction/RemoteControl/RemoteControl.uplugin`.
- Module `WebRemoteControl` (Runtime) with `PlatformAllowList: ["Mac","Win64","Linux"]`
  (VERIFIED lines 41–49) — confirms our Win11 + Linux target.
- This is the generic REST/WebSocket "call any UFUNCTION / set any UPROPERTY / exec console"
  layer. It is NOT an MCP server, but it is the most robust first-party *programmatic* control
  channel and works at runtime (not just in-editor). It is the right tool for scripted, headless
  manipulation and for any custom bridge we might author later. There is no MCP front-end for it
  in-box, so for an MCP client it is strictly a fallback/secondary to `ModelContextProtocol`.

### A5. `PythonScriptPlugin` — Python + Remote Execution (VERIFIED on-disk)
- Path: `Engine/Plugins/Experimental/PythonScriptPlugin/PythonScriptPlugin.uplugin`.
- Descriptor facts (VERIFIED): `EnabledByDefault: false`, `IsBetaVersion: true`; module
  `PythonScriptPlugin` is `UncookedOnly` (editor/uncooked only — NOT in cooked/shipping game).
- Exposes the full `unreal` Python API in-editor and a "Remote Execution" TCP listener (enabled
  via Project Settings → Plugins → Python → "Enable Remote Execution"). This is the foundation
  the `runreal/unreal-mcp` fallback drives. Editor-time tooling only — fine for our build/automation
  use, irrelevant to the shipped game.

> Caveat: every option here (MCP, Remote Control, Python) is an EDITOR/automation convenience
> for *us building the game*. None ships in the cooked Kessel build. They speed authoring of
> blueprints, materials (fog/emissive), lighting, and the sight HUD — they are not gameplay.

---

## Part B — Community MCP bridge survey

Legend — 5.8 risk: LOW = no engine-version-pinned C++; MED = Python/Node only but version
checks; HIGH = ships a C++ engine plugin pinned to ≤5.7 (recompile/ABI risk + needs MSVC).

| Project | Mechanism | UE versions claimed | Custom C++ plugin? | Capabilities | Maint. | 5.8 risk |
|---|---|---|---|---|---|---|
| **Epic `ModelContextProtocol`** (first-party) | In-editor MCP server (HTTP/SSE) + Toolsets | **5.8** (ships in-box) | No (Epic-built, precompiled) | actor spawn, lighting, material instances, Slate inspect, automation tests; extensible via toolsets | Epic, active | **LOW** |
| **runreal/unreal-mcp** | Node MCP server → Python Remote Execution | 5.4+ ("should work" 5.8) | No (uses PythonScriptPlugin) | full `unreal` Python API: spawn/edit actors, assets, console exec, arbitrary editor scripting | active 2025 | **LOW–MED** |
| **chongdashu/unreal-mcp** | C++ `UnrealMCP` plugin (TCP) + Python FastMCP server | 5.5–5.6 ("5.5+"); experimental | **Yes** | actors (create/delete/transform/query), Blueprint class+component create, BP graph nodes, input mappings, viewport | active but "experimental, not for production" | **HIGH** |
| **flopperam/unreal-engine-mcp** (now "Aura", commercial) | C++ plugin + Python server | **5.5, 5.6, 5.7** (no 5.8) | **Yes** | 64 tools: BP authoring (23+ node types), level/actor mgmt, materials, physics BP actors, VFX, UMG; autonomous "Flop Agent" | active, pivoting commercial (Aura) | **HIGH** |
| **kvick-games/UnrealMCP** | C++ plugin + Python server (blender-mcp-style) | early/unspecified | **Yes** | basic: create objects, transforms, scene info, run python scripts | low / early-stage | **HIGH** |
| **runeape-sats/unreal-mcp** | "pure python" MCP server | 5.3 | No (python) | text-driven create/manipulate 3D objects (basic) | low, pinned to 5.3 | **MED** (stale) |
| **radial-hks/MCP-Unreal-Server** | Python MCP → Python Remote Execution | generic (remote python) | No | node mgmt, remote python code exec, monitoring | low | **MED** |
| **ChiR24/Unreal_mcp** | TS + C++ "Automation Bridge" plugin | recent (2025) | **Yes** | broad editor control via native C++ bridge | newer, unproven | **HIGH** |

Notes:
- The three "serious" community bridges (chongdashu, flopperam/Aura, ChiR24) all ship a
  **C++ engine plugin**. On UE 5.8 these would need recompilation against the 5.8 ABI and none
  advertises 5.8 support yet. We also literally cannot compile C++ until VS 2022 is installed.
  That makes them poor fits as a primary path right now.
- flopperam is migrating to a paid product (Aura / tryaura.dev); its free repo's future is
  uncertain — another reason not to depend on it.
- The Python/Node bridges (runreal, runeape, radial-hks) carry NO compile risk because they
  ride Epic's `PythonScriptPlugin` remote exec — only `runreal` is current (5.4+) and actively
  maintained, which is why it is the chosen fallback.

---

## Part C — Why first-party is primary (trade vs third-party)

Per StraySpark's 5.8 write-up, the official plugin is intentionally minimal vs a mature
third-party server (which claims 370+ tools, catalog-mode token savings ~95%, multi-step
single-transaction undo, deep Blueprint read-back, and production auth hardening). For Kessel
those advantages are not worth the cost:
- We need a *reliable, version-correct* authoring assistant on a brand-new 5.8 project, not a
  huge tool surface. Official + selected toolsets covers actors, lighting, materials, physics,
  and UMG — exactly the Milestone 1 needs (fog material, emissive lights, tank rig, sight HUD).
- Zero maintenance / zero compile / zero 5.8 risk dominates "more tools" for a solo project.
- The fallback (runreal Python) fills any capability gap with arbitrary `unreal` API scripting,
  which is strictly more general than any fixed tool list.

---

## Part D — Concrete setup

### D1. PRIMARY — first-party Unreal MCP (do this once the project exists in-editor)
Prereqs: a UE 5.8 project open in the editor (and later, VS 2022 if the project has C++ modules —
note the MCP plugin itself is precompiled by Epic and does not require us to compile it).

1. Editor → Edit → Plugins. Enable **Unreal MCP** (`ModelContextProtocol`). Accept the
   auto-enable of **Toolset Registry**.
2. Enable **All Toolsets** (`AllToolsets`) for the full surface, OR enable only the toolsets you
   want (recommended for Kessel M1: `EditorToolset`, `PhysicsToolsets`, `UMGToolSet`,
   `SlateInspectorToolset`, plus material/Niagara toolsets). Restart the editor when prompted.
3. (Optional, default is fine) Confirm the server binds `http://127.0.0.1:8000/mcp`
   (Project/Editor settings → Unreal MCP). Keep loopback-only.
4. In the editor console (the `~` Cmd box / Output Log cmd line) run:
   `ModelContextProtocol.GenerateClientConfig ClaudeCode`
   This writes `.mcp.json` to the project root for Claude Code auto-discovery.
5. Launch Claude Code from the project root (`C:/shop/tank_simulator` once the `.uproject`
   lives there) so it reads `.mcp.json`.
6. Equivalent explicit registration (if you prefer to register manually instead of the generated
   `.mcp.json`). The server is Streamable-HTTP, so register it as an HTTP transport:
   ```
   claude mcp add --transport http unreal-mcp http://127.0.0.1:8000/mcp
   ```
   (ASSUMPTION: exact `claude mcp add` HTTP flag spelling — verify with `claude mcp add --help`;
   the generated `.mcp.json` from step 4 is the authoritative, supported path and is preferred.)

### D2. FALLBACK — runreal/unreal-mcp (Python Remote Execution). Requires Node.js (ABSENT now — install later).
1. Editor → Edit → Plugins → enable **Python Editor Script Plugin** (`PythonScriptPlugin`). Restart.
2. Project Settings → Plugins → Python → check **Enable Remote Execution** (and confirm the
   multicast/bind settings; default `127.0.0.1` is fine for local).
3. Install Node.js (not yet present on this machine), then register the npm-published server:
   ```
   claude mcp add unreal-python -- npx -y @runreal/unreal-mcp
   ```
   (ASSUMPTION: exact npx package invocation — verify against the repo README / npm page
   `@runreal/unreal-mcp` before relying on it. Transport is stdio, so no `--transport` flag.)
4. With the editor open and Remote Execution on, the server connects over Unreal's Python
   remote-exec TCP and exposes the full `unreal` API to the agent.

> Security: both servers grant the agent full editor control and bind to localhost with no auth.
> Keep them loopback-only; do not expose the port. Review agent-proposed changes before approving.

---

## Part E — Open questions / to verify in-editor later (do NOT do this phase)
- Exact default port + whether `ModelContextProtocol.GenerateClientConfig` emits HTTP-transport
  `.mcp.json` keys Claude Code's current build understands (verify after first editor launch).
- Whether any Milestone-1 task (armor penetration material/data, volumetric fog material graph,
  emissive light setup) needs a tool absent from the first-party toolsets, forcing the Python
  fallback. Likely material-graph node editing is the first gap.
- Confirm `claude mcp add --transport http` syntax on the installed Claude Code build.
- Whether enabling `AllToolsets` bloats the tool list enough to hurt; if so, prune to the
  minimal toolset set above.

## Sources (web, post-cutoff)
- Epic 5.8 docs — "Unreal MCP in Unreal Editor": https://dev.epicgames.com/documentation/unreal-engine/unreal-mcp-in-unreal-editor
- StraySpark — official vs third-party 5.8 MCP: https://www.strayspark.studio/blog/epic-official-mcp-plugin-ue5-8-vs-third-party
- chongdashu/unreal-mcp: https://github.com/chongdashu/unreal-mcp
- flopperam/unreal-engine-mcp (Aura): https://github.com/flopperam/unreal-engine-mcp ; https://www.flopperam.com/mcp
- kvick-games/UnrealMCP: https://github.com/kvick-games/UnrealMCP
- runreal/unreal-mcp: https://github.com/runreal/unreal-mcp ; https://www.npmjs.com/package/@runreal/unreal-mcp
- runeape-sats/unreal-mcp: https://github.com/runeape-sats/unreal-mcp
- radial-hks/MCP-Unreal-Server: https://github.com/radial-hks/mcp-unreal-server
- ChiR24/Unreal_mcp: https://github.com/ChiR24/Unreal_mcp
