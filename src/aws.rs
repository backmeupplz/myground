use serde::Deserialize;
use serde::Serialize;
use utoipa::ToSchema;

use crate::error::AppError;

/// Admin AWS credentials provided by the user — `Deserialize` only, never `Serialize`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AwsSetupRequest {
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
}

/// Result of automatic S3 + IAM setup.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AwsSetupResult {
    pub bucket_name: String,
    pub repository: String,
    pub s3_access_key: String,
    pub s3_secret_key: String,
    pub iam_user_name: String,
}

const IAM_USER_NAME: &str = "myground-backup";

/// Create an S3 bucket and restricted IAM user for backups.
///
/// The caller provides temporary admin credentials; this function creates:
/// 1. An S3 bucket `myground-backups-{random}`
/// 2. An IAM user `myground-backup` with a policy scoped to that bucket
/// 3. An access key for the restricted user
///
/// Admin credentials are dropped when this function returns.
pub async fn setup_s3_backup(req: AwsSetupRequest) -> Result<AwsSetupResult, AppError> {
    if req.access_key.trim().is_empty() {
        return Err(AppError::Io("AWS access key is required".into()));
    }
    if req.secret_key.trim().is_empty() {
        return Err(AppError::Io("AWS secret key is required".into()));
    }
    if req.region.trim().is_empty() {
        return Err(AppError::Io("AWS region is required".into()));
    }

    let creds = aws_sdk_s3::config::Credentials::new(
        &req.access_key,
        &req.secret_key,
        None,
        None,
        "myground-admin",
    );
    let region = aws_sdk_s3::config::Region::new(req.region.clone());

    // Use aws_config loader so the SDK gets a proper HTTP client,
    // but override credentials and region (never use env/profile).
    let sdk_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .no_credentials()   // disable env/profile fallback first
        .credentials_provider(creds)  // then set our explicit creds
        .region(region)
        .load()
        .await;

    let s3 = aws_sdk_s3::Client::new(&sdk_config);
    let iam = aws_sdk_iam::Client::new(&sdk_config);

    let bucket_name = create_bucket(&s3, &req.region).await?;
    create_iam_user(&iam).await?;
    attach_bucket_policy(&iam, &bucket_name).await?;
    let (access_key, secret_key) = create_access_key(&iam).await?;

    let repository = format!("s3:https://s3.{}.amazonaws.com/{}", req.region, bucket_name);

    Ok(AwsSetupResult {
        bucket_name,
        repository,
        s3_access_key: access_key,
        s3_secret_key: secret_key,
        iam_user_name: IAM_USER_NAME.to_string(),
    })
}

/// Try to create a bucket, retrying up to 3 times with new random names.
async fn create_bucket(
    s3: &aws_sdk_s3::Client,
    region: &str,
) -> Result<String, AppError> {
    use rand::Rng;

    for attempt in 0..3 {
        let suffix: u32 = rand::rng().random();
        let name = format!("myground-backups-{:08x}", suffix);

        let mut builder = s3.create_bucket().bucket(&name);

        // us-east-1 must NOT have a LocationConstraint
        if region != "us-east-1" {
            let constraint = aws_sdk_s3::types::BucketLocationConstraint::from(region);
            let location = aws_sdk_s3::types::CreateBucketConfiguration::builder()
                .location_constraint(constraint)
                .build();
            builder = builder.create_bucket_configuration(location);
        }

        match builder.send().await {
            Ok(_) => return Ok(name),
            Err(err) => {
                let service_err = err.as_service_error();
                if let Some(e) = service_err {
                    let code = e.meta().code().unwrap_or_default();
                    if code == "BucketAlreadyOwnedByYou" {
                        return Ok(name);
                    }
                    if code == "BucketAlreadyExists" {
                        if attempt < 2 {
                            continue;
                        }
                        return Err(AppError::Io(
                            "Failed to create bucket after 3 attempts (name collisions)".into(),
                        ));
                    }
                }
                return Err(AppError::Io(format!("Failed to create S3 bucket: {err}")));
            }
        }
    }
    unreachable!()
}

/// Create the restricted IAM user, skipping if it already exists.
async fn create_iam_user(iam: &aws_sdk_iam::Client) -> Result<(), AppError> {
    match iam.create_user().user_name(IAM_USER_NAME).send().await {
        Ok(_) => Ok(()),
        Err(err) => {
            let code = err
                .as_service_error()
                .and_then(|e| e.meta().code())
                .unwrap_or_default();
            if code == "EntityAlreadyExists" {
                return Ok(());
            }
            Err(AppError::Io(format!("Failed to create IAM user: {err}")))
        }
    }
}

/// Attach an inline policy granting access to only the specified bucket.
async fn attach_bucket_policy(
    iam: &aws_sdk_iam::Client,
    bucket: &str,
) -> Result<(), AppError> {
    let policy = serde_json::json!({
        "Version": "2012-10-17",
        "Statement": [
            {
                "Effect": "Allow",
                "Action": [
                    "s3:ListBucket",
                    "s3:GetBucketLocation"
                ],
                "Resource": format!("arn:aws:s3:::{bucket}")
            },
            {
                "Effect": "Allow",
                "Action": [
                    "s3:GetObject",
                    "s3:PutObject",
                    "s3:DeleteObject"
                ],
                "Resource": format!("arn:aws:s3:::{bucket}/*")
            }
        ]
    });

    iam.put_user_policy()
        .user_name(IAM_USER_NAME)
        .policy_name("myground-backup-policy")
        .policy_document(policy.to_string())
        .send()
        .await
        .map_err(|e| AppError::Io(format!("Failed to attach IAM policy: {e}")))?;

    Ok(())
}

/// Create an access key for the restricted IAM user.
async fn create_access_key(
    iam: &aws_sdk_iam::Client,
) -> Result<(String, String), AppError> {
    let result = iam
        .create_access_key()
        .user_name(IAM_USER_NAME)
        .send()
        .await
        .map_err(|e| {
            let code = e
                .as_service_error()
                .and_then(|se| se.meta().code())
                .unwrap_or_default();
            if code == "LimitExceeded" {
                AppError::Io(
                    "IAM user already has 2 access keys (AWS limit). \
                     Delete an existing key in the AWS console first."
                        .into(),
                )
            } else {
                AppError::Io(format!("Failed to create access key: {e}"))
            }
        })?;

    let key = result
        .access_key()
        .ok_or_else(|| AppError::Io("AWS returned no access key".into()))?;

    Ok((
        key.access_key_id().to_string(),
        key.secret_access_key().to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_is_not_serialize() {
        // AwsSetupRequest deliberately has no Serialize derive.
        fn _assert_deserialize<T: serde::de::DeserializeOwned>() {}
        _assert_deserialize::<AwsSetupRequest>();
    }

    #[test]
    fn result_round_trips() {
        let result = AwsSetupResult {
            bucket_name: "myground-backups-abc123".into(),
            repository: "s3:https://s3.us-east-1.amazonaws.com/myground-backups-abc123".into(),
            s3_access_key: "AKIAIOSFODNN7EXAMPLE".into(),
            s3_secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            iam_user_name: "myground-backup".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: AwsSetupResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.bucket_name, result.bucket_name);
        assert_eq!(parsed.repository, result.repository);
    }

    #[tokio::test]
    async fn empty_access_key_rejected() {
        let req = AwsSetupRequest {
            access_key: "".into(),
            secret_key: "secret".into(),
            region: "us-east-1".into(),
        };
        let err = setup_s3_backup(req).await.unwrap_err();
        assert!(err.to_string().contains("access key"));
    }

    #[tokio::test]
    async fn empty_secret_key_rejected() {
        let req = AwsSetupRequest {
            access_key: "AKIA1234".into(),
            secret_key: "".into(),
            region: "us-east-1".into(),
        };
        let err = setup_s3_backup(req).await.unwrap_err();
        assert!(err.to_string().contains("secret key"));
    }

    #[tokio::test]
    async fn empty_region_rejected() {
        let req = AwsSetupRequest {
            access_key: "AKIA1234".into(),
            secret_key: "secret".into(),
            region: "  ".into(),
        };
        let err = setup_s3_backup(req).await.unwrap_err();
        assert!(err.to_string().contains("region"));
    }
}
