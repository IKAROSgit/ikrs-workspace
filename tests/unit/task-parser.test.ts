import { describe, it, expect } from "vitest";
import { parseTasksMd, renderTasksMd } from "@/lib/task-parser";
import type { Task } from "@/types";

const SAMPLE_MD = `# BLR WORLD — Tasks

## To Do

- [ ] **Review brand refresh proposal** \`p1\` \`#drive\` \`due:2026-04-15\`
  - [ ] Download latest deck from Drive
  - [ ] Prepare feedback notes
- [ ] **Schedule Q2 planning session** \`p2\` \`#meeting\` \`due:2026-04-12\`

## In Progress

- [/] **Draft client onboarding email** \`p1\` \`#email\`
  - [x] Research BLR tone of voice
  - [ ] Write first draft

## Done

- [x] **Set up Drive folder structure** \`p3\` \`#drive\`
`;

describe("parseTasksMd", () => {
  it("parses all tasks from markdown", () => {
    const tasks = parseTasksMd(SAMPLE_MD, "eng-1");
    expect(tasks).toHaveLength(4);
  });

  it("extracts task titles", () => {
    const tasks = parseTasksMd(SAMPLE_MD, "eng-1");
    expect(tasks[0].title).toBe("Review brand refresh proposal");
    expect(tasks[2].title).toBe("Draft client onboarding email");
  });

  it("maps checkbox states to task status", () => {
    const tasks = parseTasksMd(SAMPLE_MD, "eng-1");
    expect(tasks[0].status).toBe("todo");
    expect(tasks[2].status).toBe("in_progress");
    expect(tasks[3].status).toBe("done");
  });

  it("extracts priority tags", () => {
    const tasks = parseTasksMd(SAMPLE_MD, "eng-1");
    expect(tasks[0].priority).toBe("p1");
    expect(tasks[1].priority).toBe("p2");
    expect(tasks[3].priority).toBe("p3");
  });

  it("extracts hashtag tags", () => {
    const tasks = parseTasksMd(SAMPLE_MD, "eng-1");
    expect(tasks[0].tags).toEqual(["#drive"]);
    expect(tasks[2].tags).toEqual(["#email"]);
  });

  it("extracts due dates", () => {
    const tasks = parseTasksMd(SAMPLE_MD, "eng-1");
    expect(tasks[0].dueDate).toEqual(new Date("2026-04-15"));
    expect(tasks[2].dueDate).toBeUndefined();
  });

  it("extracts subtasks", () => {
    const tasks = parseTasksMd(SAMPLE_MD, "eng-1");
    expect(tasks[0].subtasks).toHaveLength(2);
    expect(tasks[0].subtasks[0]).toEqual({
      title: "Download latest deck from Drive",
      done: false,
    });
    expect(tasks[2].subtasks[1]).toEqual({
      title: "Write first draft",
      done: false,
    });
    expect(tasks[2].subtasks[0].done).toBe(true);
  });

  it("sets engagementId on all tasks", () => {
    const tasks = parseTasksMd(SAMPLE_MD, "eng-1");
    tasks.forEach((t) => expect(t.engagementId).toBe("eng-1"));
  });
});

describe("renderTasksMd", () => {
  it("round-trips: parse then render produces equivalent markdown", () => {
    const tasks = parseTasksMd(SAMPLE_MD, "eng-1");
    const rendered = renderTasksMd(tasks, "BLR WORLD");
    const reparsed = parseTasksMd(rendered, "eng-1");
    expect(reparsed).toHaveLength(tasks.length);
    reparsed.forEach((t, i) => {
      expect(t.title).toBe(tasks[i].title);
      expect(t.status).toBe(tasks[i].status);
      expect(t.priority).toBe(tasks[i].priority);
      expect(t.tags).toEqual(tasks[i].tags);
      expect(t.subtasks).toEqual(tasks[i].subtasks);
    });
  });

  it("groups tasks by status section", () => {
    const tasks = parseTasksMd(SAMPLE_MD, "eng-1");
    const rendered = renderTasksMd(tasks, "BLR WORLD");
    expect(rendered).toContain("## To Do");
    expect(rendered).toContain("## In Progress");
    expect(rendered).toContain("## Done");
  });
});
