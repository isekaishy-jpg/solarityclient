# Stock architecture inventory

`BuildStockInventory.ps1` converts the Ghidra source-file, RTTI, and import
reports into deterministic ownership tables. It fails if any recovered
artifact lacks an owner. The reports are generated outside the repository
because the underlying client executable is locally supplied evidence.

```powershell
./tools/architecture/BuildStockInventory.ps1 `
  -EvidenceDirectory C:/path/to/ghidra/evidence `
  -OutputDirectory C:/path/to/generated/inventory
```

`TestStockSeedTopology.ps1` then proves that every distinct owner in all three
tables has a folder-backed Rust module, that every facade is declared by its
parent, and that the explicit cross-cutting seeds are present.

`NewStockFileSeedPatch.ps1` deterministically derives the internal source files
and external test modules from those ownership tables. It prints an
`apply_patch` patch rather than modifying the worktree itself. Once the seed is
complete, its output contains only the patch start and end markers.

```powershell
./tools/architecture/NewStockFileSeedPatch.ps1 `
  -InventoryDirectory C:/path/to/generated/inventory
```

```powershell
./tools/architecture/TestStockSeedTopology.ps1 `
  -InventoryDirectory C:/path/to/generated/inventory
```

The generated tests are intentionally outside production `src` trees. The
topology check covers the folder facades, evidence-derived implementation
files, dependency boundaries, cross-cutting files, and external test locations
without turning unimplemented stock behavior into placeholder Rust APIs or
collocated tests.
