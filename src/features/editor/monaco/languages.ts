import type { ExtensionConfig } from "monaco-languageclient/vscodeApiWrapper";

/** Language configuration for Terraform/HCL (comments, brackets, auto-closing). */
const hclLanguageConfiguration = JSON.stringify({
  comments: { lineComment: "#", blockComment: ["/*", "*/"] },
  brackets: [
    ["{", "}"],
    ["[", "]"],
    ["(", ")"],
  ],
  autoClosingPairs: [
    { open: "{", close: "}" },
    { open: "[", close: "]" },
    { open: "(", close: ")" },
    { open: '"', close: '"', notIn: ["string"] },
  ],
  surroundingPairs: [
    ["{", "}"],
    ["[", "]"],
    ["(", ")"],
    ['"', '"'],
  ],
});

/** YAML-like language configuration for the `ansible` language. */
const ansibleLanguageConfiguration = JSON.stringify({
  comments: { lineComment: "#" },
  brackets: [
    ["{", "}"],
    ["[", "]"],
  ],
  autoClosingPairs: [
    { open: "{", close: "}" },
    { open: "[", close: "]" },
    { open: '"', close: '"', notIn: ["string"] },
    { open: "'", close: "'", notIn: ["string"] },
  ],
  indentationRules: { increaseIndentPattern: ":\\s*$", decreaseIndentPattern: "^\\s*$" },
});

/** `ansible` reuses the YAML grammar (from the YAML default extension). */
const ansibleGrammar = JSON.stringify({
  scopeName: "source.ansible",
  patterns: [{ include: "source.yaml" }],
});

/**
 * Local VS Code extension contributing Terraform/HCL languages with the vendored
 * hashicorp/syntax grammars (MPL-2.0, see src/assets/grammars/hashicorp-syntax).
 */
export const hclExtension: ExtensionConfig = {
  config: {
    name: "iac-studio-hcl",
    publisher: "iac-studio",
    version: "1.0.0",
    engines: { vscode: "*" },
    contributes: {
      languages: [
        {
          id: "terraform",
          aliases: ["Terraform", "OpenTofu"],
          extensions: [".tf", ".tfvars", ".tofu"],
          configuration: "./language-configuration.json",
        },
        {
          id: "hcl",
          aliases: ["HCL"],
          extensions: [".hcl"],
          configuration: "./language-configuration.json",
        },
        {
          // set per file for Ansible YAML (ansible-language-server only serves this id)
          id: "ansible",
          aliases: ["Ansible"],
          configuration: "./ansible-language-configuration.json",
        },
      ],
      grammars: [
        {
          language: "ansible",
          scopeName: "source.ansible",
          path: "./syntaxes/ansible.tmGrammar.json",
        },
        {
          language: "terraform",
          scopeName: "source.hcl.terraform",
          path: "./syntaxes/terraform.tmGrammar.json",
        },
        { language: "hcl", scopeName: "source.hcl", path: "./syntaxes/hcl.tmGrammar.json" },
      ],
    },
  },
  filesOrContents: new Map<string, string | URL>([
    ["/language-configuration.json", hclLanguageConfiguration],
    ["/ansible-language-configuration.json", ansibleLanguageConfiguration],
    ["/syntaxes/ansible.tmGrammar.json", ansibleGrammar],
    [
      "/syntaxes/terraform.tmGrammar.json",
      new URL(
        "../../../assets/grammars/hashicorp-syntax/terraform.tmGrammar.json",
        import.meta.url,
      ),
    ],
    [
      "/syntaxes/hcl.tmGrammar.json",
      new URL("../../../assets/grammars/hashicorp-syntax/hcl.tmGrammar.json", import.meta.url),
    ],
  ]),
};
