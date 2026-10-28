import { beforeAll, describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { createI18n } from "vue-i18n";
import en from "../../locales/en.json";
import TrustDialog from "./TrustDialog.vue";

const i18n = createI18n({ legacy: false, locale: "en", messages: { en } });

beforeAll(() => {
  // jsdom does not implement the modal dialog API (showModal/close)
  Object.assign(HTMLDialogElement.prototype, {
    showModal(this: HTMLDialogElement) {
      this.open = true;
    },
    close(this: HTMLDialogElement) {
      this.open = false;
    },
  });
});

describe("TrustDialog", () => {
  const mountDialog = () =>
    mount(TrustDialog, {
      props: { projectName: "demo", root: "/home/u/demo" },
      global: { plugins: [i18n] },
    });

  it("explains the risk and emits the decision", async () => {
    const w = mountDialog();
    expect(w.text()).toContain("Do you trust the project “demo”?");
    expect(w.text()).toContain("local-exec");
    const [restricted, trust] = w.findAll("button");
    await trust?.trigger("click");
    expect(w.emitted("trust")).toHaveLength(1);
    await restricted?.trigger("click");
    expect(w.emitted("restricted")).toHaveLength(1);
  });

  it("Escape (cancel) means restricted, never trusted", async () => {
    const w = mountDialog();
    await w.find("dialog").trigger("cancel");
    expect(w.emitted("restricted")).toHaveLength(1);
    expect(w.emitted("trust")).toBeUndefined();
  });
});
