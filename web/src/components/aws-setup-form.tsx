import { useState } from "preact/hooks";
import { api, type AwsSetupResult } from "../api";
import { Field } from "./field";

const AWS_REGIONS = [
  "us-east-1",
  "us-east-2",
  "us-west-1",
  "us-west-2",
  "eu-west-1",
  "eu-west-2",
  "eu-west-3",
  "eu-central-1",
  "eu-north-1",
  "ap-southeast-1",
  "ap-southeast-2",
  "ap-northeast-1",
  "ap-northeast-2",
  "ap-south-1",
  "ca-central-1",
  "sa-east-1",
];

interface Props {
  onSuccess: (result: AwsSetupResult) => void;
  /** Current S3 repository URL, e.g. "s3:https://s3.us-east-1.amazonaws.com/myground-backups-abc123" */
  currentRepository?: string;
}

function bucketFromRepo(repo: string): string | null {
  // "s3:https://s3.REGION.amazonaws.com/BUCKET" → "BUCKET"
  const m = repo.match(/amazonaws\.com\/([^/]+)/);
  return m ? m[1] : null;
}

export function AwsSetupForm({ onSuccess, currentRepository }: Props) {
  const [accessKey, setAccessKey] = useState("");
  const [secretKey, setSecretKey] = useState("");
  const [region, setRegion] = useState("us-east-1");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [done, setDone] = useState(false);
  const [showSetup, setShowSetup] = useState(false);

  const handleSubmit = async () => {
    setLoading(true);
    setError("");
    try {
      const result = await api.awsSetup({
        access_key: accessKey,
        secret_key: secretKey,
        region,
      });
      setAccessKey("");
      setSecretKey("");
      setDone(true);
      onSuccess(result);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "AWS setup failed");
    } finally {
      setLoading(false);
    }
  };

  const isConfigured = done || (!!currentRepository && !showSetup);
  const bucket = currentRepository ? bucketFromRepo(currentRepository) : null;

  if (isConfigured) {
    return (
      <div class="space-y-2">
        <div class="flex gap-2 bg-green-900/30 border border-green-700/50 rounded p-3">
          <span class="text-green-400 shrink-0">&#10003;</span>
          <div class="text-sm text-green-300">
            <p>S3 cloud backups configured.</p>
            {bucket && (
              <p class="text-xs text-green-400/70 mt-1 font-mono">
                Bucket: {bucket}
              </p>
            )}
            {currentRepository && !bucket && (
              <p class="text-xs text-green-400/70 mt-1 font-mono truncate">
                {currentRepository}
              </p>
            )}
          </div>
        </div>
        <button
          type="button"
          class="text-xs text-gray-500 hover:text-gray-400"
          onClick={() => { setDone(false); setShowSetup(true); }}
        >
          Set up a new S3 bucket
        </button>
      </div>
    );
  }

  return (
    <div class="space-y-3">
      <div class="bg-gray-800/50 rounded p-3 space-y-2">
        <div class="flex gap-2">
          <span class="text-blue-400 shrink-0" aria-hidden="true">&#9432;</span>
          <p class="text-xs text-gray-400">
            MyGround will create an S3 bucket and a restricted IAM user for
            backups, then discard your admin credentials. You need an AWS
            access key with <strong class="text-gray-300">S3</strong> and{" "}
            <strong class="text-gray-300">IAM</strong> permissions.
          </p>
        </div>
        <details class="ml-5">
          <summary class="text-xs text-amber-400 cursor-pointer hover:text-amber-300">
            How do I get AWS credentials? (step-by-step)
          </summary>
          <ol class="mt-2 text-xs text-gray-400 space-y-2 list-decimal list-inside">
            <li>
              Go to{" "}
              <a
                href="https://console.aws.amazon.com/iam/"
                target="_blank"
                rel="noopener noreferrer"
                class="text-amber-400 hover:text-amber-300 underline"
              >
                console.aws.amazon.com/iam
              </a>{" "}
              and sign in to your AWS account.
            </li>
            <li>
              In the left sidebar, click <strong class="text-gray-300">Users</strong>,
              then click the{" "}
              <strong class="text-gray-300">Create user</strong> button.
            </li>
            <li>
              Enter a username (e.g.{" "}
              <code class="text-gray-300 bg-gray-700/50 px-1 rounded">myground-setup</code>)
              and click <strong class="text-gray-300">Next</strong>.
            </li>
            <li>
              On the permissions page, select{" "}
              <strong class="text-gray-300">Attach policies directly</strong>.
              Search for and check both of these policies:
              <ul class="mt-1 ml-4 space-y-0.5 list-disc">
                <li>
                  <code class="text-gray-300 bg-gray-700/50 px-1 rounded">AmazonS3FullAccess</code>
                </li>
                <li>
                  <code class="text-gray-300 bg-gray-700/50 px-1 rounded">IAMFullAccess</code>
                </li>
              </ul>
              Then click <strong class="text-gray-300">Next</strong>, review, and click{" "}
              <strong class="text-gray-300">Create user</strong>.
            </li>
            <li>
              Click the user you just created to open its detail page, then go to the{" "}
              <strong class="text-gray-300">Security credentials</strong> tab.
            </li>
            <li>
              Scroll down to the <strong class="text-gray-300">Access keys</strong>{" "}
              section and click{" "}
              <strong class="text-gray-300">Create access key</strong>.
            </li>
            <li>
              Select <strong class="text-gray-300">Other</strong> as the use case,
              click <strong class="text-gray-300">Next</strong>, optionally add a
              description, and click{" "}
              <strong class="text-gray-300">Create access key</strong>.
            </li>
            <li>
              Copy the <strong class="text-gray-300">Access key</strong> and{" "}
              <strong class="text-gray-300">Secret access key</strong> shown on screen
              and paste them into the fields below.
              You can also click{" "}
              <strong class="text-gray-300">Download .csv file</strong> as a backup —
              this is the only time the secret key is shown.
            </li>
          </ol>
          <p class="mt-2 text-xs text-gray-500">
            After MyGround finishes setup, you can delete the{" "}
            <code class="bg-gray-700/50 px-1 rounded">myground-setup</code> user
            from the AWS console — it won't be needed again.
          </p>
        </details>
      </div>
      <Field
        label="AWS Access Key"
        type="text"
        value={accessKey}
        placeholder="AKIA..."
        onInput={setAccessKey}
      />
      <Field
        label="AWS Secret Key"
        type="password"
        value={secretKey}
        onInput={setSecretKey}
      />
      <div>
        <label class="text-xs text-gray-500 block mb-1">Region</label>
        <select
          value={region}
          onChange={(e) => setRegion((e.target as HTMLSelectElement).value)}
          class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200"
        >
          {AWS_REGIONS.map((r) => (
            <option key={r} value={r}>
              {r}
            </option>
          ))}
        </select>
      </div>
      {error && <p class="text-red-400 text-sm">{error}</p>}
      <button
        type="button"
        disabled={loading || !accessKey.trim() || !secretKey.trim()}
        class="w-full py-2 bg-amber-600 hover:bg-amber-500 text-white font-medium text-sm rounded disabled:opacity-50"
        onClick={handleSubmit}
      >
        {loading ? "Setting up..." : "Quick Setup with AWS"}
      </button>
    </div>
  );
}
