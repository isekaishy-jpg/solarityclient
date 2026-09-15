// Exports a read-only stock performance index and explicitly selected functions.
// Generated disassembly/decompilation belongs outside version control.
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.program.model.listing.Function;
import java.io.PrintWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.charset.StandardCharsets;
import java.util.Locale;
import java.util.regex.Pattern;

public class ExportStockPerformanceEvidence extends GhidraScript {
    private static final String STOCK_SHA256 =
        "aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8";
    private static final Pattern TOPICS = Pattern.compile(
        "evtsched|client\\.cpp|gxdevice|m2shared|m2scene|m2model|maparea|mapchunk|" +
        "mapobj|mapdoodad|map\\.cpp|asyncfile|soundcache|soundengine|gxfont|" +
        "simpletop|simplescroll|texture\\.cpp|maxfps|gxvsync|cachesize|" +
        "texturecache|timingmethod|processaffinity|backgroundfps",
        Pattern.CASE_INSENSITIVE);

    // Run with -readOnly: missing explicit entries are recovered only in memory.
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length == 0) throw new IllegalArgumentException("expected output directory");
        if (!STOCK_SHA256.equalsIgnoreCase(currentProgram.getExecutableSHA256())) {
            throw new IllegalArgumentException("executable fingerprint does not match build 12340");
        }
        Path directory = Path.of(args[0]);
        Files.createDirectories(directory);
        try (PrintWriter out = new PrintWriter(directory.resolve("identity.txt").toFile())) {
            out.println("name=" + currentProgram.getName());
            out.println("sha256=" + currentProgram.getExecutableSHA256());
            out.println("language=" + currentProgram.getLanguageID());
        }
        try (PrintWriter out = new PrintWriter(directory.resolve("strings.tsv").toFile())) {
            out.println("address\ttext\treference\tfunction");
            for (var block : currentProgram.getMemory().getBlocks()) {
                if (!block.isInitialized() || block.getSize() > Integer.MAX_VALUE) continue;
                byte[] bytes = new byte[(int) block.getSize()];
                block.getBytes(block.getStart(), bytes);
                for (int start = 0; start < bytes.length && !monitor.isCancelled(); ++start) {
                    int end = start;
                    while (end < bytes.length && bytes[end] >= 32 && bytes[end] < 127) ++end;
                    if (end - start < 4 || end == bytes.length || bytes[end] != 0) {
                        start = end;
                        continue;
                    }
                    String text = new String(bytes, start, end - start, StandardCharsets.US_ASCII);
                    var address = block.getStart().add(start);
                    start = end;
                    if (!TOPICS.matcher(text).find()) continue;
                    String clean = text.replace('\t', ' ').replace('\n', ' ').replace('\r', ' ');
                    var references = getReferencesTo(address);
                    if (references.length == 0) out.println(address + "\t" + clean + "\t\t");
                    for (var reference : references) {
                        var from = reference.getFromAddress();
                        Function owner = getFunctionContaining(from);
                        out.println(address + "\t" + clean + "\t" + from + "\t" +
                            (owner == null ? "unrecovered" : owner.getEntryPoint()));
                    }
                }
            }
        }
        try (PrintWriter out = new PrintWriter(directory.resolve("wait-imports.tsv").toFile())) {
            var symbols = currentProgram.getSymbolTable().getExternalSymbols();
            while (symbols.hasNext()) {
                var symbol = symbols.next();
                String name = symbol.getName().toLowerCase(Locale.ROOT);
                if (!(name.contains("sleep") || name.contains("wait") ||
                    name.contains("thread") || name.contains("virtualalloc"))) continue;
                out.println(symbol.getName() + "\t" + symbol.getAddress());
                for (var reference : getReferencesTo(symbol.getAddress())) {
                    out.println("reference\t" + reference.getFromAddress() + "\t" +
                        getFunctionContaining(reference.getFromAddress()));
                }
            }
        }
        DecompInterface decompiler = new DecompInterface();
        decompiler.openProgram(currentProgram);
        try {
            for (int i = 1; i < args.length && !monitor.isCancelled(); ++i) {
                var address = toAddr(args[i]);
                Function function = getFunctionAt(address);
                if (function == null) {
                    if (getFunctionContaining(address) == null) {
                        disassemble(address);
                        function = createFunction(address, null);
                    }
                    if (function == null) {
                        println("unrecovered or overlapping entry " + address);
                        continue;
                    }
                }
                try (PrintWriter out = new PrintWriter(directory.resolve(args[i] + ".txt").toFile())) {
                    out.println("entry=" + address);
                    for (var ref : getReferencesTo(address)) {
                        out.println("xref=" + ref.getFromAddress() + " " +
                            getFunctionContaining(ref.getFromAddress()));
                    }
                    var result = decompiler.decompileFunction(function, 60, monitor);
                    if (result.decompileCompleted()) out.println(result.getDecompiledFunction().getC());
                    else out.println("decompilation failed: " + result.getErrorMessage());
                    out.println("DISASSEMBLY");
                    var instructions = currentProgram.getListing().getInstructions(function.getBody(), true);
                    while (instructions.hasNext()) {
                        var instruction = instructions.next();
                        out.println(instruction.getAddress() + " " + instruction);
                    }
                }
            }
        } finally {
            decompiler.dispose();
        }
    }
}
