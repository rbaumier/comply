pub fn test() {
    if let Some(os) = object_storage {
        let key = format!(
            "{}/event/{}/{event_id}",
            application_id,
            received_at.naive_utc().date(),
        );
        match timeout(
            S3_TIMEOUT,
            os.client.get_object().bucket(&os.bucket).key(&key).send(),
        )
        .await
        {
            Ok(Ok(obj)) => match timeout(S3_TIMEOUT, obj.body.collect()).await {
                Ok(Ok(ab)) => return Some(ab.to_vec()),
                Ok(Err(e)) => {
                    log_object_storage_error_with_context!(
                        "S3 GET OBJECT body collect failed",
                        error_chain = format!("{e}"),
                        object_key = &key,
                    );
                }
                Err(_) => {
                    log_object_storage_error_with_context!(
                        "S3 GET OBJECT body collect timed out",
                        error_chain = "timeout".to_string(),
                        object_key = &key,
                    );
                }
            },
            Ok(Err(e)) => {
                log_object_storage_error_with_context!(
                    "S3 GET OBJECT failed",
                    error_chain = DisplayErrorContext(&e).to_string(),
                    object_key = &key,
                );
            }
            Err(_) => {
                log_object_storage_error_with_context!(
                    "S3 GET OBJECT timed out",
                    error_chain = "timeout".to_string(),
                    object_key = &key,
                );
            }
        }
    }
}
