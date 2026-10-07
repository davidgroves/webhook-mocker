import { expect, test } from "@playwright/test";

test.describe("home page", () => {
  test("shows brand and DEV ONLY banner", async ({ page }) => {
    await page.goto("/");

    await expect(page.locator(".dev-banner")).toContainText("DEV ONLY");
    await expect(page.locator(".brand-name")).toHaveText("webhook-mocker");
    await expect(page.getByRole("button", { name: "New webhook" })).toBeVisible();
    await expect(page.locator(".brand-sub")).toContainText("127.0.0.1:5099");
  });

  test("empty state appears when there are no channels", async ({ page, request }) => {
    const list = await request.get("/api/channels");
    const channels = await list.json();
    for (const ch of channels) {
      await request.delete(`/api/channels/${ch.id}`);
    }

    await page.goto("/");
    await expect(page.locator(".empty-state h1")).toHaveText("Virtual channels");
    await expect(page.locator("#channel-list .empty")).toContainText("No channels yet");
  });

  test("serves healthz for the webServer probe", async ({ request }) => {
    const res = await request.get("/healthz");
    expect(res.status()).toBe(200);
    expect(await res.text()).toBe("ok");
  });
});
