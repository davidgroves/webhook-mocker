import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";

const fixturesDir = path.resolve(__dirname, "../../tests/fixtures");

async function createChannel(
  request: APIRequestContext,
  kind: string,
  name: string,
) {
  const res = await request.post("/api/channels", {
    data: { kind, name },
  });
  expect(res.status()).toBe(201);
  return res.json() as Promise<{
    id: string;
    webhook_url: string;
    webhook_path: string;
  }>;
}

/** Post a webhook, then open the channel page (avoids racing SSE full-page reloads). */
async function postAndOpen(
  page: Page,
  request: APIRequestContext,
  channelId: string,
  webhookPath: string,
  body: string,
  contentType = "application/json",
) {
  const res = await request.post(webhookPath, {
    headers: { "content-type": contentType },
    data: body,
  });
  await page.goto(`/ui/channel/${channelId}`);
  return res;
}

test.describe("webhook capture and UI", () => {
  test("Slack text webhook appears rendered with debug data", async ({ page, request }) => {
    const ch = await createChannel(request, "slack", `slack-${Date.now()}`);
    const marker = `hello-from-e2e-${Date.now()}`;

    const res = await postAndOpen(
      page,
      request,
      ch.id,
      ch.webhook_path,
      JSON.stringify({ text: `Deploy ok: *${marker}*` }),
    );
    expect(res.status()).toBe(200);
    expect(await res.text()).toBe("ok");

    const row = page.locator(".message-row").first();
    await expect(row.locator(".status")).toHaveText("200");
    await expect(row.locator(".summary")).toContainText(marker);

    await expect(page.locator(".msg-card .msg-body")).toContainText(marker);
    await expect(page.locator(".msg-card .msg-body strong")).toHaveText(marker);

    await page.locator('.tab[data-tab="request"]').click();
    await expect(page.locator("#tab-request")).toBeVisible();
    await expect(page.locator("#tab-request")).toContainText("POST");
    await expect(page.locator("#curl-box")).toContainText("curl -X POST");

    await page.locator('.tab[data-tab="body"]').click();
    await expect(page.locator("#tab-body")).toContainText(marker);

    await page.locator('.tab[data-tab="response"]').click();
    await expect(page.locator("#tab-response")).toContainText("200");
    await expect(page.locator("#tab-response")).toContainText("ok");
  });

  test("Slack Block Kit fixture renders header and section", async ({ page, request }) => {
    const ch = await createChannel(request, "slack", `blocks-${Date.now()}`);
    const fixture = fs.readFileSync(path.join(fixturesDir, "slack_blocks.json"), "utf8");

    const res = await postAndOpen(page, request, ch.id, ch.webhook_path, fixture);
    expect(res.status()).toBe(200);

    await expect(page.locator(".message-row .summary")).toContainText("Deploy complete");
    await expect(page.locator(".msg-card .blk-header")).toHaveText("Deploy complete");
    await expect(page.locator(".msg-card .blk-text")).toContainText("rolled out");
  });

  test("Teams Workflows adaptive card shows facts and action", async ({ page, request }) => {
    const ch = await createChannel(request, "teams-workflows", `teams-${Date.now()}`);
    const fixture = fs.readFileSync(path.join(fixturesDir, "teams_adaptive.json"), "utf8");

    const res = await postAndOpen(page, request, ch.id, ch.webhook_path, fixture);
    expect(res.status()).toBe(202);

    await expect(page.locator(".message-row .status")).toHaveText("202");
    await expect(page.locator(".msg-card .blk-header")).toHaveText("Incident opened");
    await expect(page.locator(".msg-card .field-title").filter({ hasText: "Severity" })).toBeVisible();
    await expect(page.locator(".msg-card .field-value").filter({ hasText: "SEV-2" })).toBeVisible();
    await expect(page.locator(".msg-card .blk-actions a")).toHaveAttribute(
      "href",
      "https://example.com/incidents/1",
    );
  });

  test("Teams legacy MessageCard returns body 1 and renders title", async ({
    page,
    request,
  }) => {
    const ch = await createChannel(request, "teams-legacy", `legacy-${Date.now()}`);
    const fixture = fs.readFileSync(path.join(fixturesDir, "teams_messagecard.json"), "utf8");

    const res = await postAndOpen(page, request, ch.id, ch.webhook_path, fixture);
    expect(res.status()).toBe(200);
    expect(await res.text()).toBe("1");

    await expect(page.locator(".msg-card .blk-header").first()).toContainText("CI failed");
  });

  test("generic webhook auto-creates a channel on first hit", async ({ page, request }) => {
    const token = `auto-${Date.now()}`;
    const res = await request.post(`/hooks/${token}`, {
      headers: { "content-type": "application/json" },
      data: { text: "auto-created payload" },
    });
    expect(res.status()).toBe(200);

    await page.goto("/");
    const item = page.locator(".channel-item", { hasText: token.slice(0, 8) });
    await expect(item).toBeVisible();
    await item.click();
    await expect(page.locator(".message-row .summary")).toContainText("auto-created payload");
  });

  test("clear messages empties the feed", async ({ page, request }) => {
    const ch = await createChannel(request, "slack", `clear-${Date.now()}`);
    await request.post(ch.webhook_path, {
      headers: { "content-type": "application/json" },
      data: { text: "will be cleared" },
    });

    await page.goto(`/ui/channel/${ch.id}`);
    await expect(page.locator(".message-row")).toHaveCount(1);

    page.once("dialog", (d) => d.accept());
    await page.getByRole("button", { name: "Clear" }).click();

    await expect(page.locator(".message-list .empty")).toContainText("Waiting for webhooks");
    await expect(page.locator(".message-row")).toHaveCount(0);
  });

  test("fault injection forces 429 on the next webhook", async ({ page, request }) => {
    const ch = await createChannel(request, "slack", `fault-${Date.now()}`);

    await page.goto(`/ui/channel/${ch.id}`);
    await page.getByRole("button", { name: "Faults" }).click();
    const dialog = page.locator("#faults-dialog");
    await expect(dialog).toBeVisible();
    await dialog.locator("#fault-429").check();
    await dialog.locator("#fault-retry-after").fill("3");

    const save = page.waitForResponse(
      (r) =>
        r.url().includes(`/api/channels/${ch.id}/faults`) &&
        r.request().method() === "POST" &&
        r.ok(),
    );
    await dialog.getByRole("button", { name: "Save" }).click();
    await save;

    const res = await request.post(ch.webhook_path, {
      headers: { "content-type": "application/json" },
      data: { text: "should rate limit" },
    });
    expect(res.status()).toBe(429);
    expect(res.headers()["retry-after"]).toBe("3");

    await page.goto(`/ui/channel/${ch.id}`);
    await expect(page.locator(".message-row .status")).toHaveText("429");
  });
});
