// Regenerate src-tauri/src/host_tools_defs.json from the upstream TypeScript definitions.
//
// The agent side validates host tool definitions strictly (name/label/loadMode/parameters), so the
// schema is extracted rather than hand-copied. Run after updating .research/omp-gui-git:
//   node scripts/extract-host-tools.mjs
import fs from "node:fs";
import path from "node:path";

const root = ".research/omp-gui-git/src/main/scheduler";
const out = "src-tauri/src/host_tools_defs.json";

/** Return the balanced `{...}` block starting at `start`. */
function balanced(text, start) {
  let depth = 0;
  for (let i = start; i < text.length; i++) {
    if (text[i] === "{") depth++;
    else if (text[i] === "}") {
      depth--;
      if (depth === 0) return text.slice(start, i + 1);
    }
  }
  throw new Error("unbalanced block");
}

/** Literal `const` values from a module, so references inside the schema resolve. */
function constsOf(src, into) {
  const re = /(?:export )?const (\w+)(?:\s*:[^=]+)?\s*=\s*(\[[^\]]*\]|"(?:[^"\\]|\\.)*"|`(?:[^`\\$]|\\.)*`|-?\d+)/g;
  for (const match of src.matchAll(re)) {
    try {
      into[match[1]] = eval(`(${match[2]})`);
    } catch {
      // Not a literal (call, template expression, …): the schema does not need it.
    }
  }
  return into;
}

const scope = {};
for (const file of fs.readdirSync(root).filter((name) => name.endsWith(".ts") && !name.includes(".test."))) {
  constsOf(fs.readFileSync(path.join(root, file), "utf8"), scope);
}

function extract(file, promptFile, toolName) {
  const src = fs.readFileSync(path.join(root, file), "utf8");
  const label = /label:\s*"([^"]+)"/.exec(src)?.[1] ?? toolName;
  const loadMode = /loadMode:\s*"([^"]+)"/.exec(src)?.[1] ?? "essential";
  const paramsAt = src.indexOf("parameters: {");
  if (paramsAt === -1) throw new Error(`${file}: no parameters block`);
  const paramsText = balanced(src, src.indexOf("{", paramsAt));
  const parameters = new Function(...Object.keys(scope), `return (${paramsText})`)(...Object.values(scope));
  const description = fs.readFileSync(path.join(root, promptFile), "utf8").trim();
  return { name: toolName, label, description, loadMode, parameters };
}

const defs = [
  extract("schedule-tool.ts", "prompts/schedule-task-tool.md", "schedule_task"),
  extract("manage-tasks-tool.ts", "prompts/manage-tasks-tool.md", "manage_scheduled_tasks"),
];

fs.writeFileSync(out, `${JSON.stringify(defs, null, 2)}\n`);
for (const def of defs) {
  const props = Object.keys(def.parameters.properties ?? {});
  console.log(`${def.name}: ${props.length} properties (${props.join(", ")})`);
}
