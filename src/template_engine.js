// Pure template-rendering engine, shared between the live preview (main.js)
// and the parity check (scripts/check-template-parity.mjs). This is the JS
// twin of the backend renderer in src-tauri/src/template_renderer.rs: the two
// implementations must produce byte-identical output for the same blocks and
// metadata, which tests/template_parity.json pins on both sides.
//
// Kept dependency-free: settings-sourced values (username, upload date) and
// the i18n error formatter are injected by the caller instead of read from
// globals, so a plain `node` process can exercise the engine.

export const TEMPLATE_MAX_DEPOTS = 100;
export const TEMPLATE_MAX_LENGTH = 200000;

export const TEMPLATE_SINGLE_FIELDS = [
  "game_name",
  "os",
  "branch",
  "build_datetime_utc",
  "build_id",
  // app_id/game_description/website come from job metadata.
  "app_id",
  "game_description",
  "website",
  // Scalar primary-depot tokens, usable in single-render blocks (title/version/
  // free text). The per-depot {{depot_id}}/{{manifest_id}} only resolve inside
  // the looped Depot List line; these point at the primary depot specifically.
  "primary_depot_id",
  "primary_manifest_id",
  // Settings-sourced tokens kept last so the Settings-driven chips appear at the
  // end of the chip row. username comes from the "Uploader name" setting.
  "username",
  "upload_date",
];
export const TEMPLATE_DEPOT_FIELDS = ["depot_id", "depot_name", "manifest_id"];

// Canonical OmniPacker credit line. Single source of truth on the JS side;
// the backend renderer (template_renderer.rs) has a matching constant.
export const OMNIPACKER_CREDIT =
  "Made using [url=https://github.com/elgreams/OmniPacker]OmniPacker[/url]";

export const renderTemplateString = (
  template,
  allowedFields,
  values,
  formatError,
) => {
  const source = String(template ?? "");
  const tokenRegex = /\{\{([^}]+)\}\}/g;
  const tokens = [];
  let match = tokenRegex.exec(source);
  while (match) {
    tokens.push(match[1].trim());
    match = tokenRegex.exec(source);
  }

  const invalid = tokens.filter((token) => !allowedFields.includes(token));
  if (invalid.length) {
    const unique = [...new Set(invalid)];
    return {
      error: formatError("template.error.invalidField", {
        fields: unique.join(", "),
      }),
    };
  }

  const output = source.replace(tokenRegex, (_, token) => {
    const key = token.trim();
    if (Object.prototype.hasOwnProperty.call(values, key)) {
      return String(values[key] ?? "");
    }
    return "";
  });

  return { output };
};

// Renders a block list against job metadata. `opts` injects the values that
// come from Settings rather than the job ({username}/{upload_date}) and the
// i18n error formatter (main.js passes `t`).
export const renderTemplateOutput = (blocks, metadata, opts) => {
  const { username = "", uploadDate = "", formatError } = opts;
  if (!metadata) {
    return { error: formatError("template.error.noMetadata") };
  }

  const baseValues = {
    game_name: metadata.game_name || "",
    os: metadata.os || "",
    branch: metadata.branch || "",
    build_datetime_utc: metadata.build_datetime_utc || "",
    build_id: metadata.build_id || "",
    app_id: metadata.app_id || "",
    game_description: metadata.game_description || "",
    website: metadata.website || "",
    username,
    upload_date: uploadDate,
    // Scalar primary-depot tokens, mirroring the backend TemplateMetadata so
    // preview matches job output.
    primary_depot_id: metadata.primary_depot_id || "",
    primary_manifest_id: metadata.primary_manifest_id || "",
  };
  const depots = Array.isArray(metadata.depots) ? metadata.depots : [];
  const outputParts = [];

  for (const block of blocks) {
    if (
      block.type === "title" ||
      block.type === "version" ||
      block.type === "uploaded_version"
    ) {
      const template = block.config.template || "";
      const rendered = renderTemplateString(
        template,
        TEMPLATE_SINGLE_FIELDS,
        baseValues,
        formatError,
      );
      if (rendered.error) {
        return rendered;
      }
      outputParts.push(rendered.output);
      continue;
    }

    if (block.type === "free_text") {
      const template = block.config.text || "";
      const rendered = renderTemplateString(
        template,
        TEMPLATE_SINGLE_FIELDS,
        baseValues,
        formatError,
      );
      if (rendered.error) {
        return rendered;
      }
      outputParts.push(rendered.output);
      continue;
    }

    if (block.type === "advertise_omnipacker") {
      outputParts.push(OMNIPACKER_CREDIT);
      continue;
    }

    if (block.type === "depot_list") {
      if (depots.length === 0) {
        return { error: formatError("template.error.noDepots") };
      }
      if (depots.length > TEMPLATE_MAX_DEPOTS) {
        return {
          error: formatError("template.error.depotLimit", {
            limit: TEMPLATE_MAX_DEPOTS,
          }),
        };
      }

      // Honor the block's own depot cap, matching the backend renderer
      // (which does `take(max_depots)`); previously the preview ignored it.
      const maxDepots = block.config.maxDepots ?? TEMPLATE_MAX_DEPOTS;
      const lineTemplate = block.config.lineTemplate || "";
      const lines = [];
      for (const depot of depots.slice(0, maxDepots)) {
        const depotValues = {
          depot_id: depot.depot_id || "",
          depot_name: depot.depot_name || "",
          manifest_id: depot.manifest_id || "",
        };
        const rendered = renderTemplateString(
          lineTemplate,
          TEMPLATE_DEPOT_FIELDS,
          depotValues,
          formatError,
        );
        if (rendered.error) {
          return rendered;
        }
        lines.push(rendered.output);
      }

      const title = block.config.title || "Depots";
      const useCode = Boolean(block.config.useCodeBlock);
      let depotOutput = `[spoiler=${title}]\n`;
      if (useCode) {
        depotOutput += "[code=text]";
      }
      depotOutput += lines.join("\n");
      if (useCode) {
        depotOutput += "[/code]";
      }
      depotOutput += "\n[/spoiler]";
      outputParts.push(depotOutput);
    }
  }

  let output = "";
  for (let i = 0; i < outputParts.length; i += 1) {
    const current = outputParts[i];
    const next = outputParts[i + 1];
    const currentType = blocks[i]?.type;
    const nextType = blocks[i + 1]?.type;
    output += current;
    if (next !== undefined) {
      // Match CS.RIN-style spacing between default blocks.
      let separator = "\n";
      if (currentType === "version" && nextType === "depot_list") {
        separator = "\n\n";
      } else if (currentType === "depot_list" && nextType === "uploaded_version") {
        separator = "";
      }
      output += separator;
    }
  }
  if (output.length > TEMPLATE_MAX_LENGTH) {
    return {
      error: formatError("template.error.lengthLimit", {
        limit: TEMPLATE_MAX_LENGTH,
      }),
    };
  }

  return { output };
};
