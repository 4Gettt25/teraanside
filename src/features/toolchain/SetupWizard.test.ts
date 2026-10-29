import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createI18n } from "vue-i18n";
import en from "../../locales/en.json";
import { useToolchainStore } from "../../stores/toolchain";
import SetupWizard from "./SetupWizard.vue";

const chooseFlavor = vi.fn();
vi.mock("../../lib/client", () => ({
  toolchainStatus: vi.fn(),
  toolchainSetup: vi.fn(),
  toolchainCancel: vi.fn(),
  toolchainChooseFlavor: (...args: unknown[]) => {
    chooseFlavor(...args);
    return Promise.resolve({
      detected: { terraform: true, ansible: false },
      needsFlavorChoice: false,
      tools: [],
      ready: false,
    });
  },
}));

const i18n = createI18n({ legacy: false, locale: "en", messages: { en } });

beforeAll(() => {
  Object.assign(HTMLDialogElement.prototype, {
    showModal(this: HTMLDialogElement) {
      this.open = true;
    },
    close(this: HTMLDialogElement) {
      this.open = false;
    },
  });
});

beforeEach(() => {
  setActivePinia(createPinia());
  chooseFlavor.mockReset();
});

function mountWizard() {
  return mount(SetupWizard, { props: { projectId: 7 }, global: { plugins: [i18n] } });
}

describe("SetupWizard", () => {
  it("asks for Terraform or OpenTofu and saves the choice as pin", async () => {
    const store = useToolchainStore();
    store.status = {
      detected: { terraform: true, ansible: false },
      needsFlavorChoice: true,
      tools: [{ tool: "terraformLs", version: "0.39.0", source: "default", installed: false }],
      ready: false,
    };
    const w = mountWizard();
    expect(w.text()).toContain("Which tool should this project use?");
    expect(w.text()).toContain("Business Source License");
    await w.find('input[value="terraform"]').setValue(true);
    await w.find("fieldset button").trigger("click");
    expect(chooseFlavor).toHaveBeenCalledWith(7, "terraform");
    // no setup button while the flavor is undecided
    expect(w.findAll(".actions button").map((b) => b.text())).toEqual(["Close"]);
  });

  it("lists tools with state text and shows step progress", async () => {
    const store = useToolchainStore();
    store.status = {
      detected: { terraform: true, ansible: false },
      needsFlavorChoice: false,
      tools: [
        { tool: "opentofu", version: "1.13.1", source: "projectToml", installed: true },
        { tool: "terraformLs", version: "0.39.0", source: "default", installed: false },
      ],
      ready: false,
    };
    const w = mountWizard();
    const items = w.findAll(".tools li");
    expect(items[0]?.text()).toContain("installed");
    expect(items[0]?.classes()).toContain("ok");
    expect(items[1]?.text()).toContain("not installed");
    expect(w.find(".actions .primary").text()).toBe("Set up");

    store.setup = {
      running: true,
      finished: null,
      steps: [
        {
          step: { kind: "tool", tool: "terraformLs", version: "0.39.0" },
          state: "running",
          progress: { phase: "downloading", received: 1, total: 4 },
          error: null,
          log: [],
        },
      ],
    };
    await w.vm.$nextTick();
    expect(w.find(".steps").text()).toContain("Install terraform-ls (language server) 0.39.0");
    expect(w.find(".steps").text()).toContain("25 %");
    expect(w.find(".actions").text()).toBe("Cancel");
  });
});
