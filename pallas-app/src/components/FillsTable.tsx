import { useState } from "react";
import type { FillDto } from "../types";

interface Props {
  fills: FillDto[];
}

const PAGE_SIZE = 50;
const PAGINATE_THRESHOLD = 500;

export function FillsTable({ fills }: Props) {
  const [page, setPage] = useState(0);

  if (fills.length === 0) {
    return (
      <p className="status" aria-live="polite">
        No fills recorded.
      </p>
    );
  }

  const paginated = fills.length > PAGINATE_THRESHOLD;
  const totalPages = paginated ? Math.ceil(fills.length / PAGE_SIZE) : 1;
  const safePage = Math.min(page, totalPages - 1);
  const visible = paginated
    ? fills.slice(safePage * PAGE_SIZE, safePage * PAGE_SIZE + PAGE_SIZE)
    : fills;

  return (
    <div className="table-scroll">
      <table>
        <caption>
          Trade fills ({fills.length} total
          {paginated ? `, showing ${safePage * PAGE_SIZE + 1}–${Math.min((safePage + 1) * PAGE_SIZE, fills.length)}` : ""})
        </caption>
        <thead>
          <tr>
            <th scope="col">Time</th>
            <th scope="col">Side</th>
            <th scope="col">Qty</th>
            <th scope="col">Price</th>
            <th scope="col">Fee</th>
          </tr>
        </thead>
        <tbody>
          {visible.map((f, i) => (
            <tr key={`${f.ts}-${f.side}-${i}`}>
              <td>{f.ts}</td>
              <td>{f.side}</td>
              <td>{f.qty}</td>
              <td>{f.price}</td>
              <td>{f.fee}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {paginated && (
        <div className="row actions-row table-pagination">
          <button
            type="button"
            className="secondary"
            disabled={safePage === 0}
            onClick={() => setPage((p) => Math.max(0, p - 1))}
          >
            Previous
          </button>
          <span className="status" aria-live="polite">
            Page {safePage + 1} of {totalPages}
          </span>
          <button
            type="button"
            className="secondary"
            disabled={safePage >= totalPages - 1}
            onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
          >
            Next
          </button>
        </div>
      )}
    </div>
  );
}
