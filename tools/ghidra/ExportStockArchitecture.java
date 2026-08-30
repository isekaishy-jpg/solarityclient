// Exports architecture evidence from an analyzed stock client executable.
//@category Solarity

import java.io.BufferedWriter;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.DataIterator;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionIterator;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.listing.Listing;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import ghidra.program.model.symbol.ReferenceManager;
import ghidra.program.model.symbol.Symbol;
import ghidra.program.model.symbol.SymbolIterator;
import ghidra.program.model.symbol.SymbolTable;

/**
 * Produces deterministic TSV reports used to justify Solarity's crate and module boundaries.
 *
 * <p>The report deliberately exports names, addresses, and relationships rather than decompiled
 * source. That is sufficient to make architectural decisions reviewable without checking generated
 * pseudocode or proprietary binary material into the repository.</p>
 */
public class ExportStockArchitecture extends GhidraScript {
    private static final Pattern SOURCE_FILE = Pattern.compile(
        "(?i)([A-Za-z0-9_.+\\-]+\\.(?:cxx|cpp|cc|c|hxx|hpp|hh|h))"
    );

    @Override
    public void run() throws Exception {
        String[] arguments = getScriptArgs();
        if (arguments.length != 1) {
            throw new IllegalArgumentException("expected one output-directory argument");
        }

        Path outputDirectory = new File(arguments[0]).toPath().toAbsolutePath().normalize();
        Files.createDirectories(outputDirectory);

        exportProgram(outputDirectory.resolve("program.tsv"));
        exportImports(outputDirectory.resolve("imports.tsv"));
        exportStrings(
            outputDirectory.resolve("source_files.tsv"),
            outputDirectory.resolve("rtti_types.tsv")
        );

        println("Exported stock architecture evidence to " + outputDirectory);
    }

    /** Records the identity of the analyzed program and the amount of recovered structure. */
    private void exportProgram(Path outputFile) throws Exception {
        long functionCount = 0;
        FunctionIterator functions = currentProgram.getFunctionManager().getFunctions(true);
        while (functions.hasNext() && !monitor.isCancelled()) {
            functions.next();
            functionCount++;
        }

        try (BufferedWriter writer = Files.newBufferedWriter(outputFile, StandardCharsets.UTF_8)) {
            writer.write("field\tvalue\n");
            writeRow(writer, "name", currentProgram.getName());
            writeRow(writer, "executable_format", currentProgram.getExecutableFormat());
            writeRow(writer, "language", currentProgram.getLanguageID().toString());
            writeRow(writer, "compiler", currentProgram.getCompilerSpec().getCompilerSpecID().toString());
            writeRow(writer, "image_base", currentProgram.getImageBase().toString());
            writeRow(writer, "md5", currentProgram.getExecutableMD5());
            writeRow(writer, "sha256", currentProgram.getExecutableSHA256());
            writeRow(writer, "function_count", Long.toString(functionCount));
        }
    }

    /** Records imported APIs because they provide direct evidence for platform responsibilities. */
    private void exportImports(Path outputFile) throws Exception {
        Set<String> rows = new TreeSet<>();
        SymbolTable symbolTable = currentProgram.getSymbolTable();
        SymbolIterator symbols = symbolTable.getExternalSymbols();

        while (symbols.hasNext() && !monitor.isCancelled()) {
            Symbol symbol = symbols.next();
            String namespace = symbol.getParentNamespace().getName(true);
            rows.add(tsv(namespace) + "\t" + tsv(symbol.getName()));
        }

        try (BufferedWriter writer = Files.newBufferedWriter(outputFile, StandardCharsets.UTF_8)) {
            writer.write("namespace\tsymbol\n");
            for (String row : rows) {
                writer.write(row);
                writer.newLine();
            }
        }
    }

    /**
     * Correlates embedded build paths and RTTI names with recovered functions that reference them.
     *
     * <p>Stock assert paths are particularly useful here: the compiler retained the original source
     * file names, and Ghidra can associate their references with the containing machine-code
     * functions. Multiple identical strings and references are collapsed for deterministic output.</p>
     */
    private void exportStrings(Path sourceOutput, Path rttiOutput) throws Exception {
        Listing listing = currentProgram.getListing();
        FunctionManager functionManager = currentProgram.getFunctionManager();
        ReferenceManager referenceManager = currentProgram.getReferenceManager();
        DataIterator dataItems = listing.getDefinedData(true);
        Set<String> sourceRows = new TreeSet<>();
        Map<String, Set<String>> rttiAddresses = new TreeMap<>();

        while (dataItems.hasNext() && !monitor.isCancelled()) {
            Data data = dataItems.next();
            if (!data.hasStringValue()) {
                continue;
            }

            Object value = data.getValue();
            if (!(value instanceof String)) {
                continue;
            }

            String text = (String) value;
            collectSourceRows(
                sourceRows,
                text,
                data.getAddress(),
                referenceManager,
                functionManager
            );
            collectRttiAddress(rttiAddresses, text, data.getAddress());
        }

        try (BufferedWriter writer = Files.newBufferedWriter(sourceOutput, StandardCharsets.UTF_8)) {
            writer.write(
                "source_file\tembedded_path\tstring_address\txref_address\tfunction_entry\tfunction_name\n"
            );
            for (String row : sourceRows) {
                writer.write(row);
                writer.newLine();
            }
        }

        try (BufferedWriter writer = Files.newBufferedWriter(rttiOutput, StandardCharsets.UTF_8)) {
            writer.write("rtti_name\taddresses\n");
            for (Map.Entry<String, Set<String>> entry : rttiAddresses.entrySet()) {
                writeRow(writer, entry.getKey(), String.join(",", entry.getValue()));
            }
        }
    }

    /** Adds one row per source string reference, retaining unreferenced strings as evidence. */
    private void collectSourceRows(
        Set<String> rows,
        String text,
        Address stringAddress,
        ReferenceManager referenceManager,
        FunctionManager functionManager
    ) {
        Matcher matcher = SOURCE_FILE.matcher(text);
        if (!matcher.find()) {
            return;
        }

        String sourceFile = matcher.group(1);
        ReferenceIterator references = referenceManager.getReferencesTo(stringAddress);
        boolean foundReference = false;

        while (references.hasNext()) {
            Reference reference = references.next();
            Address fromAddress = reference.getFromAddress();
            Function function = functionManager.getFunctionContaining(fromAddress);
            String functionEntry = function == null ? "" : function.getEntryPoint().toString();
            String functionName = function == null ? "" : function.getName(true);
            rows.add(
                tsv(sourceFile) + "\t" + tsv(text) + "\t" + stringAddress + "\t" + fromAddress
                    + "\t" + functionEntry + "\t" + tsv(functionName)
            );
            foundReference = true;
        }

        if (!foundReference) {
            rows.add(
                tsv(sourceFile) + "\t" + tsv(text) + "\t" + stringAddress + "\t\t\t"
            );
        }
    }

    /** Collects MSVC class and struct type descriptors without attempting an unstable demangle. */
    private void collectRttiAddress(
        Map<String, Set<String>> addressesByName,
        String text,
        Address address
    ) {
        if (!text.startsWith(".?AV") && !text.startsWith(".?AU")) {
            return;
        }

        addressesByName.computeIfAbsent(text, ignored -> new TreeSet<>()).add(address.toString());
    }

    /** Writes a two-column TSV row with normalized field escaping. */
    private static void writeRow(BufferedWriter writer, String first, String second)
        throws Exception {
        writer.write(tsv(first));
        writer.write('\t');
        writer.write(tsv(second));
        writer.newLine();
    }

    /** Prevents paths and diagnostic strings from corrupting the tabular evidence format. */
    private static String tsv(String value) {
        if (value == null) {
            return "";
        }

        return value
            .replace("\\", "\\\\")
            .replace("\t", "\\t")
            .replace("\r", "\\r")
            .replace("\n", "\\n");
    }
}
