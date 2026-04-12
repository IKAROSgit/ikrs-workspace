import { describe, it, expect } from "vitest";
import { extractMcpServers } from "@/lib/mcp-utils";

describe("extractMcpServers", () => {
  it("extracts gmail, calendar, drive, obsidian from mixed tool list", () => {
    const tools = [
      "Read", "Write", "Edit", "Glob", "Grep",
      "mcp__gmail__read_message", "mcp__gmail__search_messages",
      "mcp__calendar__list_events", "mcp__drive__list_files",
      "mcp__obsidian__read_note",
    ];
    const result = extractMcpServers(tools);
    const types = result.map((s) => s.type).sort();
    expect(types).toEqual(["calendar", "drive", "gmail", "obsidian"]);
  });

  it("ignores non-MCP tools", () => {
    const tools = ["Read", "Write", "Edit", "Glob", "Grep", "WebSearch"];
    const result = extractMcpServers(tools);
    expect(result).toEqual([]);
  });

  it("deduplicates multiple tools from same server", () => {
    const tools = [
      "mcp__gmail__read_message",
      "mcp__gmail__search_messages",
      "mcp__gmail__send_message",
    ];
    const result = extractMcpServers(tools);
    expect(result).toHaveLength(1);
    expect(result[0].type).toBe("gmail");
  });

  it("returns empty array for no MCP tools", () => {
    expect(extractMcpServers([])).toEqual([]);
  });

  it("ignores unknown MCP prefixes", () => {
    const tools = ["mcp__slack__send", "mcp__notion__read"];
    expect(extractMcpServers(tools)).toEqual([]);
  });

  it("populates restartCount: 0 and status: healthy", () => {
    const tools = ["mcp__gmail__read_message"];
    const result = extractMcpServers(tools);
    expect(result[0].status).toBe("healthy");
    expect(result[0].restartCount).toBe(0);
    expect(result[0].lastPing).toBeInstanceOf(Date);
  });
});
