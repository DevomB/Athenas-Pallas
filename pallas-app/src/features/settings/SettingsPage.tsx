import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { mapUserError } from "@/lib/errorMessages";
import type { ConfigDto, CredentialsDto, SweepResultDto } from "@/types";

interface Props {
  config: ConfigDto;
  onConfigChange: (c: ConfigDto) => void;
}

export function SettingsPage({ config, onConfigChange }: Props) {
  const [creds, setCreds] = useState<CredentialsDto>({
    api_key: "",
    api_secret: "",
  });
  const [hasCreds, setHasCreds] = useState(false);
  const [systemJson, setSystemJson] = useState("{}");
  const [sweepBase, setSweepBase] = useState("backtest.toml");
  const [sweepFile, setSweepFile] = useState("sweep.toml");
  const [sweepOut, setSweepOut] = useState("target/sweep.csv");
  const [sweepResult, setSweepResult] = useState<SweepResultDto | null>(null);
  const [sweepBusy, setSweepBusy] = useState(false);
  const [sweepProgress, setSweepProgress] = useState("");

  useEffect(() => {
    invoke<CredentialsDto | null>("get_credentials")
      .then((c) => {
        if (c) {
          setCreds(c);
          setHasCreds(!!c.api_key);
        }
      })
      .catch(() => undefined);

    invoke<string | null>("load_system_config")
      .then((json) => {
        if (json) setSystemJson(json);
      })
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    const unlisten = listen<string>("sweep-progress", (e) => {
      setSweepProgress(String(e.payload));
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const bestRow = useMemo(() => {
    if (!sweepResult?.rows.length) return null;
    return [...sweepResult.rows].sort((a, b) => b.sharpe - a.sharpe)[0];
  }, [sweepResult]);

  async function saveCreds() {
    try {
      await invoke("save_credentials", { credentials: creds });
      setHasCreds(!!creds.api_key);
      toast.success("Credentials saved locally");
    } catch (e) {
      toast.error(mapUserError(e));
    }
  }

  async function saveSystemConfig() {
    try {
      await invoke("save_system_config", { json: systemJson });
      toast.success("System config saved");
    } catch (e) {
      toast.error(mapUserError(e));
    }
  }

  async function runSweep() {
    setSweepBusy(true);
    setSweepProgress("Starting sweep...");
    try {
      const result = await invoke<SweepResultDto>("run_parameter_sweep", {
        req: {
          base_config_path: sweepBase,
          sweep_path: sweepFile,
          output_path: sweepOut,
        },
      });
      setSweepResult(result);
      toast.success("Sweep complete", { description: result.output_path });
    } catch (e) {
      toast.error(mapUserError(e));
    } finally {
      setSweepBusy(false);
      setSweepProgress("");
    }
  }

  async function applyBestConfig() {
    if (!bestRow) return;
    try {
      const updated = await invoke<ConfigDto>("apply_sweep_row", {
        req: {
          base_config_path: sweepBase,
          sweep_path: sweepFile,
          row_name: bestRow.name,
        },
      });
      onConfigChange(updated);
      toast.success("Applied best sweep config", {
        description: bestRow.name,
      });
    } catch (e) {
      toast.error(mapUserError(e));
    }
  }

  return (
    <Tabs defaultValue="general">
      <TabsList>
        <TabsTrigger value="general">General</TabsTrigger>
        <TabsTrigger value="credentials">Credentials</TabsTrigger>
        <TabsTrigger value="advanced">Advanced</TabsTrigger>
      </TabsList>

      <TabsContent value="general" className="pt-4">
        <Card>
          <CardHeader>
            <CardTitle>Defaults</CardTitle>
            <CardDescription>Python and output preferences</CardDescription>
          </CardHeader>
          <CardContent className="grid gap-4 sm:grid-cols-2">
            <div className="grid gap-2">
              <Label>Python executable</Label>
              <Select
                value={config.python_exe}
                onValueChange={(v) =>
                  onConfigChange({ ...config, python_exe: v })
                }
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {["python", "py", "python3"].map((p) => (
                    <SelectItem key={p} value={p}>
                      {p}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </CardContent>
        </Card>
      </TabsContent>

      <TabsContent value="credentials" className="pt-4">
        <Card>
          <CardHeader>
            <CardTitle>Binance API</CardTitle>
            <CardDescription>
              Stored in your app data folder - never written to TOML configs
            </CardDescription>
          </CardHeader>
          <CardContent className="grid gap-4">
            <div className="grid gap-2">
              <Label>API key</Label>
              <Input
                type="password"
                value={creds.api_key}
                onChange={(e) =>
                  setCreds((c) => ({ ...c, api_key: e.target.value }))
                }
              />
            </div>
            <div className="grid gap-2">
              <Label>API secret</Label>
              <Input
                type="password"
                value={creds.api_secret}
                onChange={(e) =>
                  setCreds((c) => ({ ...c, api_secret: e.target.value }))
                }
              />
            </div>
            <Button onClick={saveCreds}>
              {hasCreds ? "Update credentials" : "Save credentials"}
            </Button>
          </CardContent>
        </Card>
      </TabsContent>

      <TabsContent value="advanced" className="flex flex-col gap-4 pt-4">
        <Card>
          <CardHeader>
            <CardTitle>SystemConfig JSON</CardTitle>
            <CardDescription>
              Multi-instrument paper configuration (barter-style)
            </CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-3">
            <Textarea
              className="min-h-40 font-mono text-xs"
              value={systemJson}
              onChange={(e) => setSystemJson(e.target.value)}
              placeholder='{"instruments": [...], "executions": [...]}'
            />
            <Button variant="secondary" onClick={saveSystemConfig}>
              Save system config
            </Button>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Parameter sweep</CardTitle>
            <CardDescription>Grid search over TOML parameters</CardDescription>
          </CardHeader>
          <CardContent className="grid gap-4">
            <div className="grid gap-2 sm:grid-cols-3">
              <div className="grid gap-2">
                <Label>Base config</Label>
                <Input
                  value={sweepBase}
                  onChange={(e) => setSweepBase(e.target.value)}
                />
              </div>
              <div className="grid gap-2">
                <Label>Sweep TOML</Label>
                <Input
                  value={sweepFile}
                  onChange={(e) => setSweepFile(e.target.value)}
                />
              </div>
              <div className="grid gap-2">
                <Label>Output CSV</Label>
                <Input
                  value={sweepOut}
                  onChange={(e) => setSweepOut(e.target.value)}
                />
              </div>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button disabled={sweepBusy} onClick={runSweep}>
                {sweepBusy ? "Running sweep..." : "Run parameter sweep"}
              </Button>
              {bestRow && (
                <Button variant="secondary" onClick={applyBestConfig}>
                  Apply best ({bestRow.name})
                </Button>
              )}
            </div>
            {sweepBusy && (
              <div className="grid gap-2">
                <Progress value={undefined} />
                <p className="text-xs text-muted-foreground">{sweepProgress}</p>
              </div>
            )}
            {sweepResult && (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Name</TableHead>
                    <TableHead>PnL</TableHead>
                    <TableHead>Sharpe</TableHead>
                    <TableHead>Win rate</TableHead>
                    <TableHead />
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {sweepResult.rows.map((row) => (
                    <TableRow key={row.name}>
                      <TableCell>{row.name}</TableCell>
                      <TableCell>{row.pnl.toFixed(2)}</TableCell>
                      <TableCell>{row.sharpe.toFixed(3)}</TableCell>
                      <TableCell>{(row.win_rate * 100).toFixed(1)}%</TableCell>
                      <TableCell>
                        <Button
                          variant="link"
                          className="h-auto p-0"
                          onClick={async () => {
                            try {
                              const updated = await invoke<ConfigDto>(
                                "apply_sweep_row",
                                {
                                  req: {
                                    base_config_path: sweepBase,
                                    sweep_path: sweepFile,
                                    row_name: row.name,
                                  },
                                },
                              );
                              onConfigChange(updated);
                              toast.success(`Applied ${row.name}`);
                            } catch (e) {
                              toast.error(mapUserError(e));
                            }
                          }}
                        >
                          Apply
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
      </TabsContent>
    </Tabs>
  );
}

export async function loadCredentialsStatus(): Promise<boolean> {
  try {
    const c = await invoke<CredentialsDto | null>("get_credentials");
    return !!c?.api_key;
  } catch {
    return false;
  }
}
