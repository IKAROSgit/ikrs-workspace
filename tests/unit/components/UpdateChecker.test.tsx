import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";

// Must use vi.hoisted for variables referenced in vi.mock factories
const { mockCheck } = vi.hoisted(() => ({
  mockCheck: vi.fn(),
}));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(() => Promise.resolve("0.1.0")),
}));

vi.mock("@tauri-apps/plugin-updater", () => ({
  check: mockCheck,
}));

import { UpdateChecker } from "@/components/UpdateChecker";

describe("UpdateChecker", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCheck.mockResolvedValue(null);
  });

  it("displays the current app version", async () => {
    render(<UpdateChecker />);
    await waitFor(() => {
      expect(screen.getByText(/App Version: 0\.1\.0/)).toBeInTheDocument();
    });
  });

  it("shows Check for Updates button when idle", async () => {
    render(<UpdateChecker />);
    await waitFor(() => {
      expect(screen.getByText("Check for Updates")).toBeInTheDocument();
    });
  });

  it("shows update available when check finds one", async () => {
    const mockUpdate = {
      version: "0.2.0",
      downloadAndInstall: vi.fn(() => Promise.resolve()),
    };
    mockCheck.mockResolvedValue(mockUpdate);

    render(<UpdateChecker />);
    await waitFor(() => {
      expect(screen.getByText(/Update available: v0\.2\.0/)).toBeInTheDocument();
      expect(screen.getByText(/Install & Restart/)).toBeInTheDocument();
    });
  });

  it("shows error on failed manual check", async () => {
    mockCheck
      .mockResolvedValueOnce(null) // silent check on mount
      .mockRejectedValueOnce(new Error("Network error")); // manual check

    render(<UpdateChecker />);
    await waitFor(() => {
      expect(screen.getByText("Check for Updates")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Check for Updates"));
    await waitFor(() => {
      expect(screen.getByText("Failed to check for updates.")).toBeInTheDocument();
    });
  });
});
