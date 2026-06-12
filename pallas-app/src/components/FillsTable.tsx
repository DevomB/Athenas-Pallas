import type { FillDto } from "../types";

interface Props {
  fills: FillDto[];
}

export function FillsTable({ fills }: Props) {
  if (fills.length === 0) {
    return <p className="status">No fills recorded.</p>;
  }
  return (
    <table>
      <thead>
        <tr>
          <th>Time</th>
          <th>Side</th>
          <th>Qty</th>
          <th>Price</th>
          <th>Fee</th>
        </tr>
      </thead>
      <tbody>
        {fills.map((f, i) => (
          <tr key={i}>
            <td>{f.ts}</td>
            <td>{f.side}</td>
            <td>{f.qty}</td>
            <td>{f.price}</td>
            <td>{f.fee}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
