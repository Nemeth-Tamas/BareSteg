param(
    [string]$Carrier = "carrier.bmp",
    [string]$Payload = "secret.txt",

    [double[]]$Scales = @(
        1.00,
        0.95,
        0.90,
        0.85,
        0.80,
        0.75,
        0.70,
        0.67,
        0.60,
        0.50
    ),

    [switch]$KeepFiles
)

$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Drawing

$RepoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path

function Remove-BareStegTestFiles {
    Get-ChildItem `
        -Path $RepoRoot `
        -Filter "hidden-test*.bmp" `
        -File `
        -ErrorAction SilentlyContinue |
        Remove-Item -Force -ErrorAction SilentlyContinue

    Get-ChildItem `
        -Path $RepoRoot `
        -Filter "recovered-test*.txt" `
        -File `
        -ErrorAction SilentlyContinue |
        Remove-Item -Force -ErrorAction SilentlyContinue
}

function Resize-Bmp24 {
    param(
        [Parameter(Mandatory = $true)]
        [string]$InputFile,

        [Parameter(Mandatory = $true)]
        [string]$OutputFile,

        [Parameter(Mandatory = $true)]
        [double]$Scale
    )

    $source = [System.Drawing.Image]::FromFile($InputFile)

    try {
        $width = [Math]::Max(
            1,
            [int][Math]::Round($source.Width * $Scale)
        )

        $height = [Math]::Max(
            1,
            [int][Math]::Round($source.Height * $Scale)
        )

        $destination = [System.Drawing.Bitmap]::new(
            $width,
            $height,
            [System.Drawing.Imaging.PixelFormat]::Format24bppRgb
        )

        try {
            $graphics = [System.Drawing.Graphics]::FromImage($destination)

            try {
                $graphics.CompositingMode =
                    [System.Drawing.Drawing2D.CompositingMode]::SourceCopy

                $graphics.CompositingQuality =
                    [System.Drawing.Drawing2D.CompositingQuality]::HighQuality

                $graphics.InterpolationMode =
                    [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic

                $graphics.SmoothingMode =
                    [System.Drawing.Drawing2D.SmoothingMode]::HighQuality

                $graphics.PixelOffsetMode =
                    [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality

                $graphics.DrawImage(
                    $source,
                    [System.Drawing.Rectangle]::new(
                        0,
                        0,
                        $width,
                        $height
                    )
                )
            }
            finally {
                $graphics.Dispose()
            }

            $destination.Save(
                $OutputFile,
                [System.Drawing.Imaging.ImageFormat]::Bmp
            )
        }
        finally {
            $destination.Dispose()
        }
    }
    finally {
        $source.Dispose()
    }
}

function Test-BytesEqual {
    param(
        [Parameter(Mandatory = $true)]
        [string]$First,

        [Parameter(Mandatory = $true)]
        [string]$Second
    )

    if (-not (Test-Path $Second)) {
        return $false
    }

    $left = [System.IO.File]::ReadAllBytes($First)
    $right = [System.IO.File]::ReadAllBytes($Second)

    if ($left.Length -ne $right.Length) {
        return $false
    }

    for ($index = 0; $index -lt $left.Length; $index++) {
        if ($left[$index] -ne $right[$index]) {
            return $false
        }
    }

    return $true
}

Push-Location $RepoRoot

try {
    Remove-BareStegTestFiles

    $carrierPath = (Resolve-Path $Carrier).Path
    $payloadPath = (Resolve-Path $Payload).Path
    $hiddenPath = Join-Path $RepoRoot "hidden-test.bmp"

    Write-Host ""
    Write-Host "Building BareSteg..." -ForegroundColor Cyan

    cargo build --quiet

    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed"
    }

    $exe = Join-Path $RepoRoot "target\debug\baresteg.exe"

    Write-Host "Creating test carrier..." -ForegroundColor Cyan

    & $exe hide $carrierPath $payloadPath $hiddenPath

    if ($LASTEXITCODE -ne 0) {
        throw "BareSteg hide failed"
    }

    $results = @()

    foreach ($scale in $Scales) {
        $label = "{0:N0}" -f ($scale * 100)

        $testImage = Join-Path $RepoRoot "hidden-test-$label.bmp"
        $recovered = Join-Path $RepoRoot "recovered-test-$label.txt"

        if ($scale -eq 1.0) {
            Copy-Item $hiddenPath $testImage
        }
        else {
            Resize-Bmp24 `
                -InputFile $hiddenPath `
                -OutputFile $testImage `
                -Scale $scale
        }

        $image = [System.Drawing.Image]::FromFile($testImage)

        try {
            $dimensions = "$($image.Width)x$($image.Height)"
        }
        finally {
            $image.Dispose()
        }

        $rawOutput = (
            & $exe reveal $testImage $recovered 2>&1 |
                Out-String
        ).Trim()

        $exitCode = $LASTEXITCODE

        $qimStep = $null
        $headerMinority = $null
        $headerDisputed = $null
        $eccMinority = $null
        $eccDisputed = $null

        if ($rawOutput -match "QIM decode step: (\d+)") {
            $qimStep = [int]$Matches[1]
        }

        if (
            $rawOutput -match
            "Header ECC votes: (\d+)/(\d+).*across (\d+)/(\d+)"
        ) {
            $headerMinority = [int]$Matches[1]
            $headerDisputed = [int]$Matches[3]
        }

        if (
            $rawOutput -match
            "(?m)^ECC votes: (\d+)/(\d+).*across (\d+)/(\d+)"
        ) {
            $eccMinority = [int]$Matches[1]
            $eccDisputed = [int]$Matches[3]
        }

        $errorText = ""

        if ($rawOutput -match "BareSteg error: ([^\r\n]+)") {
            $errorText = $Matches[1]
        }

        $matches = (
            $exitCode -eq 0 -and
            (Test-BytesEqual $payloadPath $recovered)
        )

        $status = if ($matches) {
            "PASS"
        }
        else {
            "FAIL"
        }

        $results += [PSCustomObject]@{
            Scale = "$label%"
            Dimensions = $dimensions
            Result = $status
            QimStep = $qimStep
            HeaderMinority = $headerMinority
            HeaderDisputed = $headerDisputed
            EccMinority = $eccMinority
            EccDisputed = $eccDisputed
            Error = $errorText
        }
    }

    Write-Host ""
    Write-Host "BareSteg resize torture results" -ForegroundColor Cyan
    Write-Host ""

    $results |
        Format-Table `
            Scale,
            Dimensions,
            Result,
            QimStep,
            HeaderMinority,
            HeaderDisputed,
            EccMinority,
            EccDisputed,
            Error `
            -AutoSize
}
finally {
    if (-not $KeepFiles) {
        Remove-BareStegTestFiles
        Write-Host ""
        Write-Host "Test files cleaned." -ForegroundColor DarkGray
    }
    else {
        Write-Host ""
        Write-Host "Test files kept in repo root." -ForegroundColor Yellow
    }

    Pop-Location
}