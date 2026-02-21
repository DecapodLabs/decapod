#!/usr/bin/env node
const fs = require("node:fs");
const path = require("node:path");
const { runValidateEnvelope } = require(path.join(__dirname, "..", "sdk", "typescript", "decapodShim.js"));

function main() {
  const fixtureFlag = process.argv.indexOf("--fixture");
  if (fixtureFlag !== -1 && process.argv[fixtureFlag + 1]) {
    const raw = fs.readFileSync(process.argv[fixtureFlag + 1], "utf-8");
    const data = JSON.parse(raw);
    console.log(JSON.stringify({ demo: "typescript", status: data.status || "unknown" }, null, 2));
    return 0;
  }

  const env = runValidateEnvelope();
  console.log(
    JSON.stringify(
      {
        demo: "typescript",
        status: env.status,
        exit_code: env.exit_code,
      },
      null,
      2,
    ),
  );
  return 0;
}

process.exit(main());
