import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import type { FileContents } from "@/ipc";
import { ROW_PAGE } from "./csvTable";
import { TableDiff } from "./TableDiff";

const file = (
  original: string | null,
  modified: string | null,
  extra: Partial<FileContents> = {},
) => ({
  data: { original, modified, binary: false, tooLarge: false, ...extra },
  isError: false,
  error: null,
});

function show(original: string | null, modified: string | null, path = "data/prices.csv") {
  const onTextView = vi.fn();
  render(<TableDiff contents={file(original, modified)} path={path} onTextView={onTextView} />);
  return { onTextView };
}

const rowsOf = (kind: string) =>
  screen.getByTestId("table-diff").querySelectorAll(`tbody tr[data-kind="${kind}"]`);

describe("TableDiff", () => {
  beforeAll(() => {
    initI18n("en");
  });

  it("shows added, removed and changed rows with the old value of a changed cell", () => {
    show(
      "id,name,qty\n1,apple,3\n2,pear,5\n3,plum,7\n",
      "id,name,qty\n1,apple,4\n3,plum,7\n4,kiwi,1\n",
    );
    expect(screen.getByTestId("table-stats")).toHaveTextContent(
      "1 row added, 1 row removed, 1 row changed (1 cells)",
    );
    expect(rowsOf("added")).toHaveLength(1);
    expect(rowsOf("removed")).toHaveLength(1);
    const changed = screen.getByTitle("Before: 3");
    expect(changed).toHaveTextContent("34");
    expect(within(changed).getByText("3")).toHaveClass("line-through");
    // Unchanged rows are hidden by default and come back with the toggle.
    expect(screen.getByText("1 unchanged row")).toBeInTheDocument();
    expect(rowsOf("same")).toHaveLength(0);
  });

  it("toggles unchanged rows, the header row and the key column", async () => {
    const user = userEvent.setup();
    show("id,name\n1,apple\n2,pear\n", "id,name\n2,pear\n1,apple\n3,fig\n");
    expect(screen.getByRole("combobox", { name: "Match rows by" })).toHaveValue("0");
    expect(screen.getByText("key")).toBeInTheDocument();
    expect(rowsOf("added")).toHaveLength(1);

    await user.click(screen.getByLabelText("Only changed rows"));
    expect(rowsOf("same")).toHaveLength(2);

    // By position the moved rows no longer pair up: only one of them still lines up.
    await user.selectOptions(screen.getByRole("combobox", { name: "Match rows by" }), "position");
    expect(rowsOf("same")).toHaveLength(1);

    await user.click(screen.getByLabelText("First row is the header"));
    expect(screen.getByRole("columnheader", { name: /^Column 1/ })).toBeInTheDocument();
  });

  it("marks added and removed columns", () => {
    show("id,a,b\n1,x,y\n", "id,b,c\n1,y,z\n");
    expect(screen.getByRole("columnheader", { name: "a" })).toHaveAttribute(
      "data-status",
      "removed",
    );
    expect(screen.getByRole("columnheader", { name: "c" })).toHaveAttribute("data-status", "added");
    expect(screen.getByTestId("table-stats")).toHaveTextContent("1 column added, 1 column removed");
  });

  it("says when only the formatting changed", () => {
    show("id,name\n1,apple\n", 'id,name\r\n1,"apple"\r\n');
    expect(screen.getByRole("status")).toHaveTextContent(
      "Only the formatting changed (line endings, quotes). Every value is the same.",
    );
    expect(rowsOf("changed")).toHaveLength(0);
  });

  it("warns when the chosen key repeats", async () => {
    const user = userEvent.setup();
    show("id,group\n1,a\n2,a\n", "id,group\n1,a\n2,b\n");
    await user.selectOptions(screen.getByRole("combobox", { name: "Match rows by" }), "1");
    expect(
      screen.getByText(/Some values in the key column are empty or repeated/),
    ).toBeInTheDocument();
  });

  it("lists a new file's rows without the pairing controls", () => {
    show(null, "id\tv\n1\ta\n2\tb\n", "x.tsv");
    expect(rowsOf("added")).toHaveLength(2);
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    expect(screen.getByTestId("table-stats")).toHaveTextContent("2 rows");
  });

  it("renders a page of rows and shows more on request", async () => {
    const user = userEvent.setup();
    const body = Array.from({ length: ROW_PAGE + 20 }, (_, i) => `${i},v${i}`).join("\n");
    show(null, `id,v\n${body}\n`);
    expect(rowsOf("added")).toHaveLength(ROW_PAGE);
    await user.click(screen.getByRole("button", { name: "Show 20 more" }));
    expect(rowsOf("added")).toHaveLength(ROW_PAGE + 20);
  });

  it("offers the text diff for a file that is too large", async () => {
    const user = userEvent.setup();
    const onTextView = vi.fn();
    render(
      <TableDiff
        contents={file(null, null, { tooLarge: true })}
        path="big.csv"
        onTextView={onTextView}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Show text diff" }));
    expect(onTextView).toHaveBeenCalled();
  });

  it("says when nothing changed", () => {
    show("a,b\n1,2\n", "a,b\n1,2\n");
    expect(screen.getByText("No values changed.")).toBeInTheDocument();
  });
});
