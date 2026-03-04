import * as esbuild from "esbuild";

await esbuild.build({
  entryPoints: ["src/index.ts"],
  bundle: true,
  platform: "node",
  target: "node22",
  format: "esm",
  outfile: "dist/terrarium-mcp.js",
  banner: {
    js: "#!/usr/bin/env node",
  },
  // Mark nothing as external — bundle everything into a single file
});

console.log("Built dist/terrarium-mcp.js");
