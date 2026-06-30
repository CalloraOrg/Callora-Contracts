import * as doctor from "../scripts/storage-ttl-doctor";
import { SorobanRpc, xdr, nativeToScVal } from "@stellar/stellar-sdk";

describe("Storage TTL Doctor Utility Tests", () => {
  let mockExit: jest.SpyInstance;
  let mockLog: jest.SpyInstance;
  let mockSimulateTransaction: jest.SpyInstance;

  beforeEach(() => {
    mockExit = jest.spyOn(process, "exit").mockImplementation(() => {
      throw new Error("process.exit called");
    });
    mockLog = jest.spyOn(console, "log").mockImplementation(() => {});
    mockSimulateTransaction = jest.spyOn(SorobanRpc.Server.prototype, "simulateTransaction");
  });

  afterEach(() => {
    mockExit.mockRestore();
    mockLog.mockRestore();
    mockSimulateTransaction.mockRestore();
  });

  // 1. CLI argument parsing
  test("CLI argument parsing works with all flags", () => {
    const args = [
      "--threshold", "1000",
      "--rpc-url", "https://localhost:8000",
      "--vault-id", "CDVAULT123",
      "--settlement-id", "CDSETTLEMENT123",
      "--revenue-pool-id", "CDPOOL123",
      "--request-ids", "req1,req2",
      "--developer-addresses", "addr1,addr2"
    ];
    const opts = doctor.parseArgs(args);
    expect(opts.threshold).toBe(1000);
    expect(opts.rpcUrl).toBe("https://localhost:8000");
    expect(opts.vaultId).toBe("CDVAULT123");
    expect(opts.settlementId).toBe("CDSETTLEMENT123");
    expect(opts.revenuePoolId).toBe("CDPOOL123");
    expect(opts.requestIds).toEqual(["req1", "req2"]);
    expect(opts.developerAddresses).toEqual(["addr1", "addr2"]);
  });

  test("CLI argument parsing falls back to defaults for missing flags", () => {
    const opts = doctor.parseArgs([]);
    expect(opts.threshold).toBeNull();
    expect(opts.rpcUrl).toBe("https://soroban-testnet.stellar.org");
    expect(opts.vaultId).toBeNull();
    expect(opts.settlementId).toBeNull();
    expect(opts.revenuePoolId).toBeNull();
    expect(opts.requestIds).toEqual([]);
    expect(opts.developerAddresses).toEqual([]);
  });

  // Helper to create mock successful simulation results
  function mockSuccessfulSim(entries: doctor.StorageEntryTtl[]) {
    const retvalVal = nativeToScVal(entries);
    return {
      result: {
        retval: retvalVal
      }
    };
  }

  // 2. Successful report generation and grouping
  test("Successful report generation aggregates and groups categories correctly", async () => {
    // Set up mock process.argv
    process.argv = [
      "node", "scripts/storage-ttl-doctor.ts",
      "--vault-id", "CDVAULT",
      "--settlement-id", "CDSETTLEMENT",
      "--revenue-pool-id", "CDPOOL"
    ];

    // Mock simulateTransaction responses for all three contracts
    // Vault returns Instance & ProcessedRequest
    // Settlement returns Instance & DeveloperBalance
    // Pool returns Instance
    mockSimulateTransaction
      .mockResolvedValueOnce(mockSuccessfulSim([
        {
          category: "Instance",
          key_desc: "Instance",
          storage_type: "Instance",
          ttl: 500000,
          threshold: 50000,
          bump_amount: 100000
        },
        {
          category: "ProcessedRequest",
          key_desc: "ProcessedRequest",
          storage_type: "Persistent",
          ttl: 80000,
          threshold: 10000,
          bump_amount: 30000
        }
      ]))
      .mockResolvedValueOnce(mockSuccessfulSim([
        {
          category: "Instance",
          key_desc: "Instance",
          storage_type: "Instance",
          ttl: 600000,
          threshold: 50000,
          bump_amount: 100000
        },
        {
          category: "DeveloperBalance",
          key_desc: "DeveloperBalance",
          storage_type: "Persistent",
          ttl: 45000,
          threshold: 50000, // This is below default threshold!
          bump_amount: 50000
        }
      ]))
      .mockResolvedValueOnce(mockSuccessfulSim([
        {
          category: "Instance",
          key_desc: "Instance",
          storage_type: "Instance",
          ttl: 520000,
          threshold: 50000,
          bump_amount: 100000
        }
      ]));

    // We expect it to exit with 1 because DeveloperBalance (ttl=45000) is below its threshold (50000)
    await expect(doctor.run()).rejects.toThrow("process.exit called");
    expect(mockExit).toHaveBeenCalledWith(1);

    const reportJson = JSON.parse(mockLog.mock.calls[0][0]) as doctor.DoctorReport;

    expect(reportJson.errors).toHaveLength(0);
    expect(reportJson.summary.categories_below_threshold).toBe(1);
    expect(reportJson.summary.status).toBe("WARN");

    // Check grouping
    expect(reportJson.categories.Instance.status).toBe("OK");
    expect(reportJson.categories.Instance.remaining_ttl).toBe(500000); // min of 500000, 600000, 520000

    expect(reportJson.categories.ProcessedRequest.status).toBe("OK");
    expect(reportJson.categories.ProcessedRequest.remaining_ttl).toBe(80000);

    expect(reportJson.categories.DeveloperBalance.status).toBe("WARN"); // 45000 < 50000
    expect(reportJson.categories.DeveloperBalance.remaining_ttl).toBe(45000);
  });

  // 3. Threshold handling (custom CLI threshold)
  test("Custom CLI threshold overrides default entry threshold", async () => {
    process.argv = [
      "node", "scripts/storage-ttl-doctor.ts",
      "--vault-id", "CDVAULT",
      "--threshold", "40000" // Lower than the entry threshold of 50000
    ];

    mockSimulateTransaction.mockResolvedValueOnce(mockSuccessfulSim([
      {
        category: "Instance",
        key_desc: "Instance",
        storage_type: "Instance",
        ttl: 45000, // below default (50000), but above custom (40000)
        threshold: 50000,
        bump_amount: 100000
      }
    ]));

    // Should succeed because 45000 > 40000
    await expect(doctor.run()).rejects.toThrow("process.exit called");
    expect(mockExit).toHaveBeenCalledWith(0);

    const reportJson = JSON.parse(mockLog.mock.calls[0][0]) as doctor.DoctorReport;
    expect(reportJson.summary.categories_below_threshold).toBe(0);
    expect(reportJson.summary.status).toBe("OK");
    expect(reportJson.categories.Instance.status).toBe("OK");
  });

  // 4. Empty categories
  test("Handles empty categories gracefully", async () => {
    process.argv = [
      "node", "scripts/storage-ttl-doctor.ts",
      "--vault-id", "CDVAULT"
    ];

    // Vault returns instance TTL only, processed request is empty
    mockSimulateTransaction.mockResolvedValueOnce(mockSuccessfulSim([
      {
        category: "Instance",
        key_desc: "Instance",
        storage_type: "Instance",
        ttl: 500000,
        threshold: 50000,
        bump_amount: 100000
      }
    ]));

    await expect(doctor.run()).rejects.toThrow("process.exit called");
    expect(mockExit).toHaveBeenCalledWith(0);

    const reportJson = JSON.parse(mockLog.mock.calls[0][0]) as doctor.DoctorReport;
    expect(reportJson.categories.ProcessedRequest.status).toBe("EMPTY");
    expect(reportJson.categories.ProcessedRequest.remaining_ttl).toBeNull();
  });

  // 5. Malformed or missing responses
  test("Gracefully handles simulation errors or missing values", async () => {
    process.argv = [
      "node", "scripts/storage-ttl-doctor.ts",
      "--vault-id", "CDVAULT"
    ];

    // Mock simulateTransaction returning a simulation error
    mockSimulateTransaction.mockResolvedValueOnce({
      error: "Contract method not found"
    });

    // Should exit with 1 because of the simulation error
    await expect(doctor.run()).rejects.toThrow("process.exit called");
    expect(mockExit).toHaveBeenCalledWith(1);

    const reportJson = JSON.parse(mockLog.mock.calls[0][0]) as doctor.DoctorReport;
    expect(reportJson.errors).toHaveLength(1);
    expect(reportJson.errors[0]).toContain("Simulation failed for Vault");
    expect(reportJson.summary.status).toBe("ERROR");
  });
});
