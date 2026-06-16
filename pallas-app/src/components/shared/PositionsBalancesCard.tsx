import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { PositionsSnapshotDto } from "@/types";

interface Props {
  snapshot: PositionsSnapshotDto | null;
}

export function PositionsBalancesCard({ snapshot }: Props) {
  if (!snapshot) {
    return (
      <Empty className="border-none py-6">
        <EmptyHeader>
          <EmptyTitle>No session data</EmptyTitle>
          <EmptyDescription>
            Start a session to see balances and positions.
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <div>
        <p className="mb-2 text-sm font-semibold">Balances</p>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Asset</TableHead>
              <TableHead>Amount</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {snapshot.balances.map((b) => (
              <TableRow key={b.asset}>
                <TableCell>{b.asset}</TableCell>
                <TableCell>{b.amount}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
      <div>
        <p className="mb-2 text-sm font-semibold">Positions</p>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Instrument</TableHead>
              <TableHead>Qty</TableHead>
              <TableHead>Mark</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {snapshot.positions.length === 0 ? (
              <TableRow>
                <TableCell colSpan={3} className="text-muted-foreground">
                  Flat
                </TableCell>
              </TableRow>
            ) : (
              snapshot.positions.map((p) => (
                <TableRow key={p.instrument}>
                  <TableCell>{p.instrument}</TableCell>
                  <TableCell>{p.qty}</TableCell>
                  <TableCell>{p.mark_price ?? "—"}</TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}
