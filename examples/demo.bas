! Sample HTBasic program — tests core language features
PRINT "=== HTBasic Interpreter Demo ==="
PRINT ""

! Arithmetic
PRINT "Arithmetic:"
PRINT "  2 + 3 = "; 2 + 3
PRINT "  10 / 3 = "; 10 / 3
PRINT "  2 ^ 8 = "; 2 ^ 8
PRINT ""

! Variables and strings
X = 42
Name$ = "HTBasic"
PRINT "Variable X = "; X
PRINT "String Name$ = "; Name$
PRINT "String length = "; LEN(Name$)
PRINT "Uppercase = "; UPC$(Name$)
PRINT ""

! Logic and comparison
PRINT "Logic:"
PRINT "  5 > 3 = "; 5 > 3
PRINT "  1 AND 0 = "; 1 AND 0
PRINT "  1 OR 0 = "; 1 OR 0
PRINT ""

! FOR loop
PRINT "FOR loop 1 to 5:"
FOR I = 1 TO 5
    PRINT "  I = "; I
NEXT I
PRINT ""

! WHILE loop
PRINT "WHILE loop:"
Count = 3
WHILE Count > 0
    PRINT "  Count = "; Count
    Count = Count - 1
END WHILE
PRINT ""

! IF/THEN/ELSE
Temperature = 75
IF Temperature > 80 THEN PRINT "It's hot!" ELSE PRINT "It's comfortable."
PRINT ""

! DATA and READ
DATA 100, 200, 300
READ A, B, C
PRINT "DATA/READ:"
PRINT "  A = "; A
PRINT "  B = "; B
PRINT "  C = "; C
PRINT ""

! GOTO
GOTO SkipLabel
PRINT "This should NOT print"
SkipLabel: PRINT "GOTO jumped successfully!"
PRINT ""

! Built-in math
PRINT "Math functions:"
PRINT "  ABS(-5) = "; ABS(-5)
PRINT "  SQR(16) = "; SQR(16)
PRINT "  SIN(0) = "; SIN(0)
PRINT "  PI = "; PI
PRINT ""

PRINT "=== Demo Complete ==="
END
