import { expect, test } from "@playwright/test";

test.describe("channel management", () => {
  test("creates a Slack channel from the New webhook dialog", async ({ page }) => {
    const name = `alerts-${Date.now()}`;
    await page.goto("/");

    await page.getByRole("button", { name: "New webhook" }).click();
    const dialog = page.locator("#new-dialog");
    await expect(dialog).toBeVisible();

    await dialog.locator("#new-kind").selectOption("slack");
    await dialog.locator("#new-name").fill(name);
    await dialog.getByRole("button", { name: "Create" }).click();

    await expect(page).toHaveURL(/\/ui\/channel\/[0-9a-f-]+/);
    await expect(page.locator(".feed-header h1")).toHaveText(name);
    await expect(page.locator("#webhook-url")).toContainText("/slack/services/");
    await expect(page.locator(".channel-item.active .channel-name")).toContainText(name);
    await expect(page.locator(".message-list .empty")).toContainText("Waiting for webhooks");
  });

  test("creates Teams and generic channels via API and shows them in the sidebar", async ({
    page,
    request,
  }) => {
    const teamsName = `ops-${Date.now()}`;
    const genericName = `catch-${Date.now()}`;

    const teams = await request.post("/api/channels", {
      data: { kind: "teams-workflows", name: teamsName },
    });
    expect(teams.status()).toBe(201);
    const teamsBody = await teams.json();

    const generic = await request.post("/api/channels", {
      data: { kind: "generic", name: genericName },
    });
    expect(generic.status()).toBe(201);

    await page.goto("/");
    await expect(page.locator(".channel-item", { hasText: teamsName })).toBeVisible();
    await expect(page.locator(".channel-item", { hasText: genericName })).toBeVisible();

    await page.locator(".channel-item", { hasText: teamsName }).click();
    await expect(page.locator(".feed-header h1")).toHaveText(teamsName);
    await expect(page.locator("#webhook-url")).toHaveText(teamsBody.webhook_url);
  });

  test("deletes a channel from the UI", async ({ page, request }) => {
    const name = `doomed-${Date.now()}`;
    const created = await request.post("/api/channels", {
      data: { kind: "generic", name },
    });
    const { id } = await created.json();

    await page.goto(`/ui/channel/${id}`);
    await expect(page.locator(".feed-header h1")).toHaveText(name);

    page.once("dialog", (d) => d.accept());
    await page.getByRole("button", { name: "Delete" }).click();

    await expect(page).toHaveURL("/");
    await expect(page.locator(".channel-item", { hasText: name })).toHaveCount(0);
  });
});
