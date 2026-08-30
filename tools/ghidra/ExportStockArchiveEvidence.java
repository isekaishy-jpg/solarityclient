// Exports MPQ catalog strings and their references from the analyzed stock client.
//@category Solarity

import java.io.BufferedWriter;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Set;
import java.util.TreeSet;

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.DataIterator;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.listing.Listing;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import ghidra.program.model.symbol.ReferenceManager;

/** Produces deterministic, non-decompiled evidence for stock MPQ discovery behavior. */
public class ExportStockArchiveEvidence extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] arguments = getScriptArgs();
        if (arguments.length != 1) {
            throw new IllegalArgumentException("expected one output-file argument");
        }

        Path outputFile = new File(arguments[0]).toPath().toAbsolutePath().normalize();
        Files.createDirectories(outputFile.getParent());

        Listing listing = currentProgram.getListing();
        FunctionManager functions = currentProgram.getFunctionManager();
        ReferenceManager references = currentProgram.getReferenceManager();
        DataIterator dataItems = listing.getDefinedData(true);
        Set<String> rows = new TreeSet<>();

        while (dataItems.hasNext() && !monitor.isCancelled()) {
            Data data = dataItems.next();
            if (!data.hasStringValue() || !(data.getValue() instanceof String)) {
                continue;
            }

            String text = (String) data.getValue();
            if (!text.toLowerCase().contains(".mpq")) {
                continue;
            }

            Address stringAddress = data.getAddress();
            ReferenceIterator xrefs = references.getReferencesTo(stringAddress);
            boolean foundReference = false;
            while (xrefs.hasNext()) {
                Reference reference = xrefs.next();
                Address fromAddress = reference.getFromAddress();
                Function function = functions.getFunctionContaining(fromAddress);
                String functionEntry = function == null ? "" : function.getEntryPoint().toString();
                String functionName = function == null ? "" : function.getName(true);
                rows.add(
                    escape(text) + "\t" + stringAddress + "\t" + fromAddress + "\t"
                        + functionEntry + "\t" + escape(functionName)
                );
                foundReference = true;
            }
            if (!foundReference) {
                rows.add(escape(text) + "\t" + stringAddress + "\t\t\t");
            }
        }

        try (BufferedWriter writer = Files.newBufferedWriter(outputFile, StandardCharsets.UTF_8)) {
            writer.write("string\tstring_address\txref_address\tfunction_entry\tfunction_name\n");
            for (String row : rows) {
                writer.write(row);
                writer.newLine();
            }
        }
        println("Exported stock archive evidence to " + outputFile);
    }

    /** Escapes control characters so each recovered string remains one TSV field. */
    private static String escape(String value) {
        return value
            .replace("\\", "\\\\")
            .replace("\t", "\\t")
            .replace("\r", "\\r")
            .replace("\n", "\\n");
    }
}
