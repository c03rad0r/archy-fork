import { test } from "node:test";
import assert from "node:assert/strict";
import { parseClientsHtml } from "../src/datum/parse.js";

test("multiple miners of one model keep their worker names and full identities", () => {
  const row = (username: string, agent: string) => "<tr>" +
    ["0", "192.0.2.3", username, "Subscribed", "2 s", "16384", "32768 (2)", "0 (0) 0%", "1.2 Th/s (1 s)", "", agent]
      .map(value => `<td>${value}</td>`).join("") + "</tr>";
  const miners = parseClientsHtml('<table><tr><th>Auth Username</th></tr>' +
    row("addressA.kitchen", "bitaxe/BM1368") + row("addressA.garage", "bitaxe/BM1368") + row("addressB.kitchen", "unknown") + '</table>');
  assert.equal(miners.length, 3);
  assert.deepEqual(miners.map(m => m.nickname), ["kitchen", "garage", "kitchen"]);
  assert.equal(new Set(miners.map(m => m.authUsername)).size, 3);
  assert.ok(miners.every(m => m.location === "unassigned"));
});
