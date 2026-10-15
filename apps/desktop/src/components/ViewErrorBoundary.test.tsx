import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { ViewErrorBoundary } from "./ViewErrorBoundary";

function Boom({ fail }: { fail: boolean }) {
  if (fail) throw new Error("Illegal value for token color: #fff");
  return <p>fine</p>;
}

describe("ViewErrorBoundary", () => {
  it("shows the error with a close button and recovers on a new reset key", async () => {
    initI18n("vi");
    vi.spyOn(console, "error").mockImplementation(() => {});
    const onClose = vi.fn();
    const { rerender } = render(
      <ViewErrorBoundary onClose={onClose} resetKey="a">
        <Boom fail />
      </ViewErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Không hiển thị được khung này");
    expect(screen.getByRole("alert")).toHaveTextContent("#fff");
    await userEvent.setup().click(screen.getByRole("button", { name: "Đóng" }));
    expect(onClose).toHaveBeenCalled();

    rerender(
      <ViewErrorBoundary onClose={onClose} resetKey="b">
        <Boom fail={false} />
      </ViewErrorBoundary>,
    );
    expect(screen.getByText("fine")).toBeInTheDocument();
    vi.restoreAllMocks();
  });
});
