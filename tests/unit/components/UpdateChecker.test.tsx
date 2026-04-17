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

  // --- Phase 4c §3 Layer 2 downgrade-protection integration tests ------
  // These prove that UpdateChecker actually invokes the version-compare
  // guard and that a future refactor removing the guard would fail a
  // test, not silently regress.
  describe("Layer 2 downgrade protection", () => {
    it("rejects a check result whose version is lower than current (no 'Update available' UI)", async () => {
      // Current app version mocked as 0.3.0; manifest claims 0.1.0
      const { getVersion } = await import("@tauri-apps/api/app");
      vi.mocked(getVersion).mockResolvedValue("0.3.0");

      const staleUpdate = {
        version: "0.1.0",
        downloadAndInstall: vi.fn(() => Promise.resolve()),
      };
      mockCheck.mockResolvedValue(staleUpdate);

      render(<UpdateChecker />);
      // Allow mount-time silent check + render cycle.
      await waitFor(() => {
        expect(screen.getByText(/App Version: 0\.3\.0/)).toBeInTheDocument();
      });

      // No "Update available" message should ever appear.
      expect(screen.queryByText(/Update available/)).not.toBeInTheDocument();
      expect(screen.queryByText(/Install & Restart/)).not.toBeInTheDocument();
      // downloadAndInstall must not have been invoked — the guard
      // fires before any install attempt.
      expect(staleUpdate.downloadAndInstall).not.toHaveBeenCalled();
    });

    it("rejects a check result whose version equals current (same-version replay)", async () => {
      const { getVersion } = await import("@tauri-apps/api/app");
      vi.mocked(getVersion).mockResolvedValue("0.2.0");

      const sameVersion = {
        version: "0.2.0",
        downloadAndInstall: vi.fn(() => Promise.resolve()),
      };
      mockCheck.mockResolvedValue(sameVersion);

      render(<UpdateChecker />);
      await waitFor(() => {
        expect(screen.getByText(/App Version: 0\.2\.0/)).toBeInTheDocument();
      });

      expect(screen.queryByText(/Update available/)).not.toBeInTheDocument();
      expect(sameVersion.downloadAndInstall).not.toHaveBeenCalled();
    });

    it("accepts a check result whose version is strictly higher than current", async () => {
      const { getVersion } = await import("@tauri-apps/api/app");
      vi.mocked(getVersion).mockResolvedValue("0.1.0");

      const newer = {
        version: "0.2.0",
        downloadAndInstall: vi.fn(() => Promise.resolve()),
      };
      mockCheck.mockResolvedValue(newer);

      render(<UpdateChecker />);
      await waitFor(() => {
        expect(screen.getByText(/Update available: v0\.2\.0/)).toBeInTheDocument();
      });
    });
  });
});
